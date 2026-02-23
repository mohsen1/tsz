//! Import rewriting, module specifier resolution, and auto-import candidate
//! collection helpers for code fixes.
//!
//! Extracted from `handlers_code_fixes.rs` to reduce file size. Contains all
//! import-related `Server` methods: CommonJS rewriting, declaration-file
//! type-only import normalization, fallback candidate scanning, and
//! verbatim-CommonJS auto-import codepath.

use super::Server;
use super::handlers_code_fixes_utils::{
    extract_quoted_text, find_jsdoc_import_line, import_spec_sort_key, import_specs_are_sorted,
    is_path_excluded_with_patterns, parse_inserted_import_spec, parse_named_import_line,
    relative_module_path_candidates, reorder_import_candidates_for_package_roots,
    resolve_module_path,
};
use tsz::lsp::Project;
use tsz::lsp::code_actions::{CodeActionProvider, ImportCandidate};
use tsz::lsp::position::LineMap;
use tsz::parser::ParserState;

impl Server {
    pub(super) fn best_import_module_specifier_for_name(
        &self,
        current_file_path: &str,
        symbol_name: &str,
        auto_import_file_exclude_patterns: &[String],
        auto_import_specifier_exclude_regexes: &[String],
        import_module_specifier_preference: Option<&str>,
    ) -> Option<String> {
        let mut files = self.open_files.clone();
        for project_files in self.external_project_files.values() {
            for path in project_files {
                if files.contains_key(path) {
                    continue;
                }
                if let Some(text) = self.open_files.get(path) {
                    files.insert(path.clone(), text.clone());
                } else if let Ok(text) = std::fs::read_to_string(path) {
                    files.insert(path.clone(), text);
                }
            }
        }
        if !files.contains_key(current_file_path)
            && let Ok(content) = std::fs::read_to_string(current_file_path)
        {
            files.insert(current_file_path.to_string(), content);
        }
        Self::add_project_config_files(&mut files, current_file_path);

        let mut project = Project::new();
        project.set_allow_importing_ts_extensions(self.allow_importing_ts_extensions);
        project.set_auto_imports_allowed_without_tsconfig(
            self.auto_imports_allowed_for_inferred_projects,
        );
        project.set_import_module_specifier_ending(
            self.completion_import_module_specifier_ending.clone(),
        );
        project.set_import_module_specifier_preference(
            import_module_specifier_preference
                .map(std::string::ToString::to_string)
                .or_else(|| self.import_module_specifier_preference.clone()),
        );
        project.set_auto_import_file_exclude_patterns(auto_import_file_exclude_patterns.to_vec());
        project.set_auto_import_specifier_exclude_regexes(
            auto_import_specifier_exclude_regexes.to_vec(),
        );
        for (path, text) in &files {
            project.set_file(path.clone(), text.clone());
        }

        let mut candidates: Vec<ImportCandidate> = project
            .get_import_candidates_for_prefix(current_file_path, symbol_name)
            .into_iter()
            .filter(|candidate| candidate.local_name == symbol_name)
            .filter(|candidate| {
                if candidate.module_specifier.starts_with('.') {
                    let path_candidates = relative_module_path_candidates(
                        current_file_path,
                        &candidate.module_specifier,
                    );
                    if path_candidates.iter().any(|path| {
                        is_path_excluded_with_patterns(path, auto_import_file_exclude_patterns)
                    }) {
                        return false;
                    }
                    if let Some(target_path) = resolve_module_path(
                        current_file_path,
                        &candidate.module_specifier,
                        &self.open_files,
                    ) {
                        return !is_path_excluded_with_patterns(
                            &target_path,
                            auto_import_file_exclude_patterns,
                        );
                    }
                    return true;
                }
                if is_path_excluded_with_patterns(
                    &candidate.module_specifier,
                    auto_import_file_exclude_patterns,
                ) {
                    return false;
                }
                let synthetic_node_modules_path =
                    format!("/node_modules/{}", candidate.module_specifier);
                !is_path_excluded_with_patterns(
                    &synthetic_node_modules_path,
                    auto_import_file_exclude_patterns,
                )
            })
            .collect();
        candidates.sort_by(|a, b| {
            let a_segments = a.module_specifier.matches('/').count();
            let b_segments = b.module_specifier.matches('/').count();
            a_segments
                .cmp(&b_segments)
                .then_with(|| a.module_specifier.len().cmp(&b.module_specifier.len()))
                .then_with(|| a.module_specifier.cmp(&b.module_specifier))
        });
        candidates.into_iter().next().map(|c| c.module_specifier)
    }

    pub(super) fn interface_symbol_import_is_usable(
        &self,
        interface_file_path: &str,
        interface_imports: &std::collections::HashMap<String, String>,
        symbol_name: &str,
        auto_import_file_exclude_patterns: &[String],
    ) -> bool {
        let Some(interface_symbol_source) = interface_imports.get(symbol_name) else {
            return true;
        };
        let Some(source_file_path) = resolve_module_path(
            interface_file_path,
            interface_symbol_source,
            &self.open_files,
        ) else {
            return false;
        };
        if !is_path_excluded_with_patterns(&source_file_path, auto_import_file_exclude_patterns) {
            return true;
        }

        let Some(source_content) = self
            .open_files
            .get(&source_file_path)
            .cloned()
            .or_else(|| std::fs::read_to_string(&source_file_path).ok())
        else {
            return false;
        };
        let source_imports =
            super::handlers_code_fixes_utils::parse_named_import_map(&source_content);
        let Some(reexport_source) = source_imports.get(symbol_name) else {
            return false;
        };
        let Some(reexport_file_path) =
            resolve_module_path(&source_file_path, reexport_source, &self.open_files)
        else {
            return false;
        };
        !is_path_excluded_with_patterns(&reexport_file_path, auto_import_file_exclude_patterns)
    }

    pub(super) fn rewrite_import_fixes_for_type_order(
        &self,
        content: &str,
        response_actions: &mut [serde_json::Value],
    ) {
        let Some(type_order) = self.organize_imports_type_order.as_deref() else {
            return;
        };
        for action in response_actions {
            Self::rewrite_single_import_fix_action(
                content,
                action,
                type_order,
                self.organize_imports_ignore_case,
            );
        }
    }

    pub(super) fn rewrite_commonjs_import_fixes(
        &self,
        file_path: &str,
        content: &str,
        response_actions: &mut [serde_json::Value],
    ) {
        if !Self::is_js_like_file(file_path) || !Self::is_commonjs_source(content) {
            return;
        }

        for action in response_actions {
            if action.get("fixName").and_then(serde_json::Value::as_str) != Some("import") {
                continue;
            }
            let Some(changes) = action
                .get_mut("changes")
                .and_then(serde_json::Value::as_array_mut)
            else {
                continue;
            };
            for file_change in changes {
                let Some(text_changes) = file_change
                    .get_mut("textChanges")
                    .and_then(serde_json::Value::as_array_mut)
                else {
                    continue;
                };
                for text_change in text_changes {
                    let Some(new_text) = text_change
                        .get("newText")
                        .and_then(serde_json::Value::as_str)
                    else {
                        continue;
                    };
                    let Some(rewritten) = Self::rewrite_single_import_to_commonjs_require(new_text)
                    else {
                        continue;
                    };
                    text_change["newText"] = serde_json::json!(rewritten);
                }
            }
        }
    }

    pub(super) fn rewrite_single_import_to_commonjs_require(new_text: &str) -> Option<String> {
        let trimmed = new_text.trim();
        if trimmed.starts_with("import type ") || !trimmed.starts_with("import ") {
            return None;
        }

        let require_stmt =
            if let Some((specs, module_specifier, _quote)) = parse_named_import_line(trimmed) {
                let rewritten_specs = specs
                    .iter()
                    .map(|spec| spec.raw.replace(" as ", ": "))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "const {{ {} }} = require(\"{}\")",
                    rewritten_specs,
                    Self::normalize_commonjs_module_specifier(&module_specifier)
                )
            } else if let Some(rest) = trimmed.strip_prefix("import * as ") {
                let (local_name, module_specifier) = rest.split_once(" from ")?;
                let module_specifier = extract_quoted_text(module_specifier)?;
                format!(
                    "const {} = require(\"{}\")",
                    local_name.trim(),
                    Self::normalize_commonjs_module_specifier(module_specifier)
                )
            } else if let Some(rest) = trimmed.strip_prefix("import ") {
                let (local_name, module_specifier) = rest.split_once(" from ")?;
                let module_specifier = extract_quoted_text(module_specifier)?;
                if local_name.contains('{') || local_name.contains('*') {
                    return None;
                }
                format!(
                    "const {} = require(\"{}\")",
                    local_name.trim(),
                    Self::normalize_commonjs_module_specifier(module_specifier)
                )
            } else {
                return None;
            };

        let leading_len = new_text.len().saturating_sub(new_text.trim_start().len());
        let trailing_len = new_text.len().saturating_sub(new_text.trim_end().len());
        Some(format!(
            "{}{}{}",
            &new_text[..leading_len],
            require_stmt,
            &new_text[new_text.len() - trailing_len..]
        ))
    }

    fn rewrite_single_import_fix_action(
        content: &str,
        action: &mut serde_json::Value,
        type_order: &str,
        ignore_case: bool,
    ) {
        if action.get("fixName").and_then(serde_json::Value::as_str) != Some("import") {
            return;
        }
        let Some(changes) = action
            .get_mut("changes")
            .and_then(serde_json::Value::as_array_mut)
        else {
            return;
        };
        if changes.len() != 1 {
            return;
        }
        let Some(file_change) = changes.get_mut(0) else {
            return;
        };
        let Some(text_changes) = file_change
            .get_mut("textChanges")
            .and_then(serde_json::Value::as_array_mut)
        else {
            return;
        };
        if text_changes.len() != 1 {
            return;
        }
        let Some(text_change) = text_changes.get_mut(0) else {
            return;
        };

        let start_line = text_change
            .get("start")
            .and_then(|v| v.get("line"))
            .and_then(serde_json::Value::as_u64)
            .map(|n| n as usize);
        let Some(start_line) = start_line else {
            return;
        };
        if start_line == 0 {
            return;
        }
        let lines: Vec<&str> = content.split('\n').collect();
        let Some(original_line) = lines.get(start_line - 1).copied() else {
            return;
        };
        let line = original_line.trim_end_matches('\r');
        let Some((mut specs, module_specifier, quote)) = parse_named_import_line(line) else {
            return;
        };

        let inserted_text = text_change
            .get("newText")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let Some(inserted_spec) = parse_inserted_import_spec(inserted_text) else {
            return;
        };
        if specs
            .iter()
            .any(|spec| spec.local_name == inserted_spec.local_name)
        {
            return;
        }

        if import_specs_are_sorted(&specs, type_order, ignore_case) {
            let inserted_key = import_spec_sort_key(&inserted_spec, type_order, ignore_case);
            let idx = specs
                .iter()
                .position(|spec| inserted_key < import_spec_sort_key(spec, type_order, ignore_case))
                .unwrap_or(specs.len());
            specs.insert(idx, inserted_spec);
        } else {
            specs.push(inserted_spec);
        }

        let joined = specs
            .iter()
            .map(|spec| spec.raw.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let rewritten_line =
            format!("import {{ {joined} }} from {quote}{module_specifier}{quote};");
        let end_offset = line.len() as u64 + 1;
        text_change["start"] = serde_json::json!({
            "line": start_line,
            "offset": 1
        });
        text_change["end"] = serde_json::json!({
            "line": start_line,
            "offset": end_offset
        });
        text_change["newText"] = serde_json::json!(rewritten_line);
    }

    pub(super) fn rewrite_jsdoc_import_fixes(
        content: &str,
        response_actions: &mut [serde_json::Value],
    ) {
        let Some((line_no, line_text, line_prefix, module_specifier, mut existing_specs)) =
            find_jsdoc_import_line(content)
        else {
            return;
        };

        for action in response_actions {
            if action.get("fixName").and_then(serde_json::Value::as_str) != Some("import") {
                continue;
            }
            let Some(changes) = action
                .get_mut("changes")
                .and_then(serde_json::Value::as_array_mut)
            else {
                continue;
            };
            if changes.len() != 1 {
                continue;
            }
            let Some(file_change) = changes.get_mut(0) else {
                continue;
            };
            let Some(text_changes) = file_change
                .get_mut("textChanges")
                .and_then(serde_json::Value::as_array_mut)
            else {
                continue;
            };
            if text_changes.len() != 1 {
                continue;
            }
            let Some(text_change) = text_changes.get_mut(0) else {
                continue;
            };
            let Some(new_text) = text_change
                .get("newText")
                .and_then(serde_json::Value::as_str)
            else {
                continue;
            };
            let Some((specs, inserted_module, _quote)) = parse_named_import_line(new_text.trim())
            else {
                continue;
            };
            if inserted_module != module_specifier {
                continue;
            }
            for spec in specs {
                if !existing_specs
                    .iter()
                    .any(|existing| existing.local_name == spec.local_name)
                {
                    existing_specs.push(spec);
                }
            }

            let joined = existing_specs
                .iter()
                .map(|spec| spec.raw.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let rewritten_line =
                format!("{line_prefix}@import {{ {joined} }} from \"{module_specifier}\"");
            let end_offset = line_text.len() as u64 + 1;
            text_change["start"] = serde_json::json!({
                "line": line_no,
                "offset": 1
            });
            text_change["end"] = serde_json::json!({
                "line": line_no,
                "offset": end_offset
            });
            text_change["newText"] = serde_json::json!(rewritten_line);
        }
    }

    fn apply_text_edits_to_source(
        source: &str,
        line_map: &LineMap,
        edits: &[tsz::lsp::rename::TextEdit],
    ) -> Option<String> {
        let mut edits_with_offsets = Vec::with_capacity(edits.len());
        for edit in edits {
            let start = line_map.position_to_offset(edit.range.start, source)? as usize;
            let end = line_map.position_to_offset(edit.range.end, source)? as usize;
            if start > end || end > source.len() {
                return None;
            }
            edits_with_offsets.push((start, end, &edit.new_text));
        }

        edits_with_offsets.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));

        let mut updated = source.to_string();
        for (start, end, new_text) in edits_with_offsets {
            updated.replace_range(start..end, new_text);
        }
        Some(updated)
    }

    pub(super) fn apply_missing_imports_fix_all(
        file_path: &str,
        content: &str,
        import_candidates: &[ImportCandidate],
    ) -> Option<String> {
        if import_candidates.is_empty() {
            return None;
        }

        let mut updated = content.to_string();
        let mut changed = false;
        for candidate in import_candidates {
            if let Some(next) =
                Self::apply_commonjs_missing_import_candidate(file_path, &updated, candidate)
            {
                updated = next;
                changed = true;
                continue;
            }

            let mut parser = ParserState::new(file_path.to_string(), updated.clone());
            let root = parser.parse_source_file();
            let arena = parser.into_arena();
            let mut binder = tsz::binder::BinderState::new();
            binder.bind_source_file(&arena, root);
            let line_map = LineMap::build(&updated);
            let provider = CodeActionProvider::new(
                &arena,
                &binder,
                &line_map,
                file_path.to_string(),
                &updated,
            );

            if let Some(edits) = provider.build_auto_import_edit(root, candidate)
                && let Some(next) = Self::apply_text_edits_to_source(&updated, &line_map, &edits)
            {
                updated = next;
                changed = true;
            }
        }

        if changed && Self::is_declaration_file_path(file_path) {
            updated = Self::normalize_declaration_file_type_only_named_imports(&updated);
        }

        changed.then_some(updated)
    }

    fn apply_commonjs_missing_import_candidate(
        file_path: &str,
        content: &str,
        candidate: &ImportCandidate,
    ) -> Option<String> {
        if candidate.is_type_only
            || !Self::is_js_like_file(file_path)
            || !Self::is_commonjs_source(content)
        {
            return None;
        }

        let module_specifier =
            Self::normalize_commonjs_module_specifier(&candidate.module_specifier);
        let binding = match &candidate.kind {
            tsz::lsp::code_actions::ImportCandidateKind::Named { export_name } => {
                if export_name == &candidate.local_name {
                    format!(
                        "const {{ {} }} = require(\"{}\")",
                        candidate.local_name, module_specifier
                    )
                } else {
                    format!(
                        "const {{ {}: {} }} = require(\"{}\")",
                        export_name, candidate.local_name, module_specifier
                    )
                }
            }
            tsz::lsp::code_actions::ImportCandidateKind::Default
            | tsz::lsp::code_actions::ImportCandidateKind::Namespace => {
                format!(
                    "const {} = require(\"{}\")",
                    candidate.local_name, module_specifier
                )
            }
        };

        if content.contains(&binding) {
            return None;
        }

        let insert_offset = if content.starts_with("#!") {
            content.find('\n').map_or(content.len(), |idx| idx + 1)
        } else {
            0
        };

        let mut updated = String::with_capacity(content.len() + binding.len() + 2);
        updated.push_str(&content[..insert_offset]);
        updated.push_str(&binding);
        if content[insert_offset..].starts_with('\n') {
            updated.push('\n');
        } else {
            updated.push_str("\n\n");
        }
        updated.push_str(&content[insert_offset..]);
        Some(updated)
    }

    pub(super) fn is_commonjs_source(content: &str) -> bool {
        content.contains("module.exports")
            || content.contains("exports.")
            || content.contains("require(")
    }

    pub(super) fn is_js_like_file(file_path: &str) -> bool {
        std::path::Path::new(file_path)
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| {
                matches!(
                    ext.to_ascii_lowercase().as_str(),
                    "js" | "jsx" | "mjs" | "cjs"
                )
            })
    }

    pub(super) fn normalize_commonjs_module_specifier(specifier: &str) -> String {
        if !(specifier.starts_with("./") || specifier.starts_with("../")) {
            return specifier.to_string();
        }

        for ext in [
            ".d.ts", ".d.mts", ".d.cts", ".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs",
            ".cjs",
        ] {
            if let Some(stripped) = specifier.strip_suffix(ext) {
                return stripped.to_string();
            }
        }

        specifier.to_string()
    }

    pub(super) fn verbatim_commonjs_auto_import_codefix_action(
        &self,
        file_path: &str,
        content: &str,
        line_map: &LineMap,
        request_span: Option<(tsz::lsp::position::Position, tsz::lsp::position::Position)>,
    ) -> Option<serde_json::Value> {
        let mut files = self.open_files.clone();
        for project_files in self.external_project_files.values() {
            for path in project_files {
                if files.contains_key(path) {
                    continue;
                }
                if let Ok(text) = std::fs::read_to_string(path) {
                    files.insert(path.clone(), text);
                }
            }
        }
        files
            .entry(file_path.to_string())
            .or_insert_with(|| content.to_string());

        if !Self::is_ts_like_file_for_codefix(file_path) {
            return None;
        }
        let (start_pos, end_pos) = request_span?;
        let start_off = line_map.position_to_offset(start_pos, content)? as usize;
        let end_off = line_map.position_to_offset(end_pos, content)? as usize;
        if end_off <= start_off || end_off > content.len() {
            return None;
        }
        let missing_name = content[start_off..end_off].trim();
        if !Self::is_identifier_for_codefix(missing_name) {
            return None;
        }

        let mut candidates: Vec<(String, String, Vec<String>)> = Vec::new();
        for text in files.values() {
            candidates.extend(Self::extract_ambient_export_equals_modules_for_codefix(
                text,
            ));
        }
        for (path, text) in &files {
            if path == file_path || !Self::is_js_like_file(path) || !text.contains("module.exports")
            {
                continue;
            }
            let Some(specifier) = Self::relative_module_specifier_for_codefix(file_path, path)
            else {
                continue;
            };
            let alias = Self::commonjs_binding_name_from_specifier_for_codefix(&specifier);
            if alias.is_empty() {
                continue;
            }
            let members = Self::extract_module_exports_object_members_for_codefix(text);
            if members.is_empty() {
                continue;
            }
            candidates.push((specifier, alias, members));
        }

        for (module_specifier, alias, members) in candidates {
            let replacement = if missing_name == alias {
                alias.clone()
            } else if members.iter().any(|m| m == missing_name) {
                format!("{alias}.{missing_name}")
            } else {
                continue;
            };

            let mut text_changes = Vec::new();
            let import_stmt = format!("import {alias} = require(\"{module_specifier}\");\n\n");
            if !content.contains(import_stmt.trim()) {
                text_changes.push(serde_json::json!({
                    "start": { "line": 1, "offset": 1 },
                    "end": { "line": 1, "offset": 1 },
                    "newText": import_stmt
                }));
            }
            text_changes.push(serde_json::json!({
                "start": { "line": start_pos.line + 1, "offset": start_pos.character + 1 },
                "end": { "line": end_pos.line + 1, "offset": end_pos.character + 1 },
                "newText": replacement
            }));

            return Some(serde_json::json!({
                "fixName": "import",
                "description": format!("Add import from \"{module_specifier}\""),
                "changes": [{
                    "fileName": file_path,
                    "textChanges": text_changes
                }]
            }));
        }

        None
    }

    fn is_ts_like_file_for_codefix(path: &str) -> bool {
        std::path::Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| {
                matches!(
                    ext.to_ascii_lowercase().as_str(),
                    "ts" | "tsx" | "mts" | "cts"
                )
            })
    }

    fn relative_module_specifier_for_codefix(from_file: &str, target_file: &str) -> Option<String> {
        let from = std::path::Path::new(from_file);
        let target = std::path::Path::new(target_file);
        let (Some(from_parent), Some(target_parent)) = (from.parent(), target.parent()) else {
            return None;
        };
        if from_parent != target_parent {
            return None;
        }
        let stem = target.file_stem()?.to_str()?;
        Some(format!("./{stem}"))
    }

    fn commonjs_binding_name_from_specifier_for_codefix(specifier: &str) -> String {
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

    fn extract_module_exports_object_members_for_codefix(content: &str) -> Vec<String> {
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
        let mut out = Vec::new();
        let mut seen = rustc_hash::FxHashSet::default();
        for line in body.lines() {
            let trimmed = line.trim();
            let Some((raw_name, _)) = trimmed.split_once(':') else {
                continue;
            };
            let name = raw_name.trim().trim_matches('"').trim_matches('\'');
            if Self::is_identifier_for_codefix(name) && seen.insert(name.to_string()) {
                out.push(name.to_string());
            }
        }
        out
    }

    fn extract_ambient_export_equals_modules_for_codefix(
        content: &str,
    ) -> Vec<(String, String, Vec<String>)> {
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
                    Self::is_identifier_for_codefix(alias).then(|| alias.to_string())
                })
                .unwrap_or_default();
            if !alias.is_empty() {
                let mut members = Vec::new();
                let mut seen = rustc_hash::FxHashSet::default();
                for line in body.lines() {
                    let trimmed = line.trim();
                    let Some(paren_idx) = trimmed.find('(') else {
                        continue;
                    };
                    let mut head = trimmed[..paren_idx].trim();
                    if head.ends_with('?') {
                        head = head.trim_end_matches('?').trim();
                    }
                    if Self::is_identifier_for_codefix(head) && seen.insert(head.to_string()) {
                        members.push(head.to_string());
                    }
                }
                modules.push((module_name.to_string(), alias, members));
            }
            cursor = idx + 1;
        }
        modules
    }

    fn is_identifier_for_codefix(text: &str) -> bool {
        let mut chars = text.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
            return false;
        }
        chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
    }

    fn is_declaration_file_path(file_path: &str) -> bool {
        let Some(file_name) = std::path::Path::new(file_path)
            .file_name()
            .and_then(|name| name.to_str())
        else {
            return false;
        };

        file_name == "d.ts"
            || file_name.ends_with(".d.ts")
            || file_name == "d.mts"
            || file_name.ends_with(".d.mts")
            || file_name == "d.cts"
            || file_name.ends_with(".d.cts")
    }

    fn declaration_file_prefers_type_only_import(content: &str, name: &str) -> bool {
        let type_usage = [
            format!(": {name}"),
            format!("<{name}>"),
            format!("extends {name}"),
            format!("implements {name}"),
            format!(" as {name}"),
        ]
        .iter()
        .any(|needle| content.contains(needle));

        if !type_usage {
            return false;
        }

        let value_usage = [
            format!("new {name}"),
            format!("{name}("),
            format!("{name}."),
            format!("typeof {name}"),
        ]
        .iter()
        .any(|needle| content.contains(needle));

        !value_usage
    }

    fn declaration_file_local_import_name(spec: &str) -> &str {
        let trimmed = spec.trim().trim_start_matches("type ").trim();
        if let Some((_, local)) = trimmed.split_once(" as ") {
            local.trim()
        } else {
            trimmed
        }
    }

    fn normalize_declaration_file_type_only_named_imports(content: &str) -> String {
        let mut normalized = String::with_capacity(content.len());
        for line in content.split_inclusive('\n') {
            let newline = if line.ends_with("\r\n") {
                "\r\n"
            } else if line.ends_with('\n') {
                "\n"
            } else {
                ""
            };
            let line_body = line.trim_end_matches(['\r', '\n']);
            let Some(open) = line_body.find('{') else {
                normalized.push_str(line_body);
                normalized.push_str(newline);
                continue;
            };
            let Some(close_rel) = line_body[open + 1..].find('}') else {
                normalized.push_str(line_body);
                normalized.push_str(newline);
                continue;
            };
            let close = open + 1 + close_rel;
            if !line_body[..open].trim_start().starts_with("import ")
                || !line_body[close..].contains(" from ")
            {
                normalized.push_str(line_body);
                normalized.push_str(newline);
                continue;
            }

            let import_prefix = line_body[..open].trim_start();
            let clause_is_type_only = import_prefix.starts_with("import type ");
            let imports = &line_body[open + 1..close];
            let mut rebuilt = Vec::new();
            for spec in imports.split(',') {
                let trimmed = spec.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if trimmed.starts_with("type ") {
                    rebuilt.push(trimmed.to_string());
                    continue;
                }
                let local_name = Self::declaration_file_local_import_name(trimmed);
                if !clause_is_type_only
                    && Self::declaration_file_prefers_type_only_import(content, local_name)
                {
                    rebuilt.push(format!("type {trimmed}"));
                } else {
                    rebuilt.push(trimmed.to_string());
                }
            }

            if rebuilt.is_empty() {
                normalized.push_str(line_body);
                normalized.push_str(newline);
                continue;
            }

            let mut rebuilt_line = String::with_capacity(line_body.len() + 8);
            rebuilt_line.push_str(&line_body[..open + 1]);
            rebuilt_line.push(' ');
            rebuilt_line.push_str(&rebuilt.join(", "));
            rebuilt_line.push(' ');
            rebuilt_line.push_str(&line_body[close..]);
            normalized.push_str(&rebuilt_line);
            normalized.push_str(newline);
        }
        normalized
    }

    pub(super) fn collect_import_candidates(
        &self,
        current_file_path: &str,
        diagnostics: &[tsz::lsp::diagnostics::LspDiagnostic],
        auto_import_file_exclude_patterns: &[String],
        auto_import_specifier_exclude_regexes: &[String],
        import_module_specifier_preference: Option<&str>,
    ) -> Vec<ImportCandidate> {
        let mut files = self.open_files.clone();
        let mut external_project_paths = rustc_hash::FxHashSet::default();
        for project_files in self.external_project_files.values() {
            for path in project_files {
                external_project_paths.insert(path.clone());
                if files.contains_key(path) {
                    continue;
                }
                if let Some(text) = self.open_files.get(path) {
                    files.insert(path.clone(), text.clone());
                } else if let Ok(text) = std::fs::read_to_string(path) {
                    files.insert(path.clone(), text);
                }
            }
        }
        if !files.contains_key(current_file_path)
            && let Ok(content) = std::fs::read_to_string(current_file_path)
        {
            files.insert(current_file_path.to_string(), content);
        }
        Self::add_project_config_files(&mut files, current_file_path);
        if files.is_empty() {
            return Vec::new();
        }
        let is_commonjs_js_file = files.get(current_file_path).is_some_and(|content| {
            Self::is_js_like_file(current_file_path) && Self::is_commonjs_source(content)
        });

        let mut project = Project::new();
        project.set_allow_importing_ts_extensions(self.allow_importing_ts_extensions);
        project.set_auto_imports_allowed_without_tsconfig(
            self.auto_imports_allowed_for_inferred_projects,
        );
        project.set_import_module_specifier_ending(
            self.completion_import_module_specifier_ending.clone(),
        );
        project.set_import_module_specifier_preference(
            import_module_specifier_preference
                .map(std::string::ToString::to_string)
                .or_else(|| self.import_module_specifier_preference.clone()),
        );
        project.set_auto_import_file_exclude_patterns(auto_import_file_exclude_patterns.to_vec());
        project.set_auto_import_specifier_exclude_regexes(
            auto_import_specifier_exclude_regexes.to_vec(),
        );
        for (path, text) in &files {
            project.set_file(path.clone(), text.clone());
        }

        let mut candidates =
            project.get_import_candidates_for_diagnostics(current_file_path, diagnostics);
        let mut fallback_names = rustc_hash::FxHashSet::default();
        for diag in diagnostics {
            if let Some(name) = Self::missing_name_from_diagnostic_message(&diag.message) {
                fallback_names.insert(name);
            }
        }
        if candidates.is_empty() {
            if fallback_names.is_empty() {
                // Preserve legacy behavior for diagnostics whose message shape does not
                // include a directly parseable missing identifier.
                candidates.extend(project.get_import_candidates_for_prefix(current_file_path, ""));
            } else {
                for missing_name in &fallback_names {
                    candidates.extend(
                        project.get_import_candidates_for_prefix(current_file_path, &missing_name),
                    );
                }
            }
        }
        if candidates.is_empty() && !fallback_names.is_empty() {
            candidates.extend(Self::fallback_import_candidates_from_export_scan(
                current_file_path,
                &files,
                &fallback_names,
            ));
        }
        if candidates.is_empty() && !fallback_names.is_empty() {
            candidates.extend(Self::fallback_import_candidates_from_side_effect_imports(
                current_file_path,
                &files,
                &fallback_names,
            ));
        }
        if candidates.is_empty() && !fallback_names.is_empty() {
            candidates.extend(Self::fallback_import_candidates_from_external_paths(
                current_file_path,
                &external_project_paths,
                &files,
                &fallback_names,
            ));
        }

        let mut seen: rustc_hash::FxHashSet<(String, String, String, bool)> =
            rustc_hash::FxHashSet::default();
        let mut deduped = Vec::with_capacity(candidates.len());

        for mut candidate in candidates.drain(..) {
            if is_commonjs_js_file
                && !matches!(
                    candidate.kind,
                    tsz::lsp::code_actions::ImportCandidateKind::Named { .. }
                )
            {
                continue;
            }
            if Self::is_js_like_file(current_file_path) {
                candidate.module_specifier =
                    Self::normalize_commonjs_module_specifier(&candidate.module_specifier);
            }
            let kind_key = match &candidate.kind {
                tsz::lsp::code_actions::ImportCandidateKind::Named { export_name } => {
                    format!("named:{export_name}")
                }
                tsz::lsp::code_actions::ImportCandidateKind::Default => "default".to_string(),
                tsz::lsp::code_actions::ImportCandidateKind::Namespace => "namespace".to_string(),
            };
            if seen.insert((
                candidate.module_specifier.clone(),
                candidate.local_name.clone(),
                kind_key,
                candidate.is_type_only,
            )) {
                deduped.push(candidate);
            }
        }

        reorder_import_candidates_for_package_roots(&mut deduped);
        deduped
    }

    fn missing_name_from_diagnostic_message(message: &str) -> Option<String> {
        let prefixes = [
            ("Cannot find name '", '\''),
            ("Cannot find name \"", '"'),
            ("Cannot find name `", '`'),
        ];
        for (prefix, terminator) in prefixes {
            if let Some(rest) = message.strip_prefix(prefix)
                && let Some(end) = rest.find(terminator)
                && end > 0
            {
                return Some(rest[..end].to_string());
            }
        }
        None
    }

    fn fallback_import_candidates_from_export_scan(
        current_file_path: &str,
        files: &rustc_hash::FxHashMap<String, String>,
        missing_names: &rustc_hash::FxHashSet<String>,
    ) -> Vec<ImportCandidate> {
        let existing_specifiers = Self::collect_existing_import_specifiers(files);
        let mut out = Vec::new();
        let mut seen = rustc_hash::FxHashSet::default();

        for (path, text) in files {
            if path == current_file_path {
                continue;
            }
            if !path.contains("/node_modules/.pnpm/") {
                continue;
            }
            for missing_name in missing_names {
                let export_patterns = [
                    format!("export declare function {missing_name}"),
                    format!("export function {missing_name}"),
                    format!("export declare const {missing_name}"),
                    format!("export const {missing_name}"),
                    format!("export declare let {missing_name}"),
                    format!("export let {missing_name}"),
                    format!("export declare class {missing_name}"),
                    format!("export class {missing_name}"),
                    format!("export declare interface {missing_name}"),
                    format!("export interface {missing_name}"),
                    format!("export type {missing_name}"),
                ];
                if !export_patterns.iter().any(|pattern| text.contains(pattern)) {
                    continue;
                }
                let Some(module_specifier) =
                    Self::module_specifier_from_node_modules_path(path, &existing_specifiers)
                else {
                    continue;
                };
                if seen.insert((module_specifier.clone(), missing_name.clone())) {
                    out.push(ImportCandidate::named(
                        module_specifier,
                        missing_name.clone(),
                        missing_name.clone(),
                    ));
                }
            }
        }

        out
    }

    fn collect_existing_import_specifiers(
        files: &rustc_hash::FxHashMap<String, String>,
    ) -> rustc_hash::FxHashSet<String> {
        let mut out = rustc_hash::FxHashSet::default();
        for text in files.values() {
            for line in text.lines() {
                if let Some(spec) = Self::extract_module_specifier_from_import_line(line) {
                    out.insert(spec);
                }
            }
        }
        out
    }

    fn fallback_import_candidates_from_side_effect_imports(
        current_file_path: &str,
        files: &rustc_hash::FxHashMap<String, String>,
        missing_names: &rustc_hash::FxHashSet<String>,
    ) -> Vec<ImportCandidate> {
        let mut out = Vec::new();
        let mut seen = rustc_hash::FxHashSet::default();
        for (path, text) in files {
            if path == current_file_path {
                continue;
            }
            for line in text.lines() {
                let trimmed = line.trim_start();
                if !(trimmed.starts_with("import \"") || trimmed.starts_with("import '")) {
                    continue;
                }
                let Some(specifier) = Self::extract_module_specifier_from_import_line(trimmed)
                else {
                    continue;
                };
                if !Self::is_bare_package_specifier(&specifier) {
                    continue;
                }
                for missing_name in missing_names {
                    if seen.insert((specifier.clone(), missing_name.clone())) {
                        out.push(ImportCandidate::named(
                            specifier.clone(),
                            missing_name.clone(),
                            missing_name.clone(),
                        ));
                    }
                }
            }
        }
        out
    }

    fn is_bare_package_specifier(specifier: &str) -> bool {
        !specifier.starts_with('.')
            && !specifier.starts_with('/')
            && !specifier.contains(':')
            && !specifier.is_empty()
    }

    fn fallback_import_candidates_from_external_paths(
        current_file_path: &str,
        external_project_paths: &rustc_hash::FxHashSet<String>,
        files: &rustc_hash::FxHashMap<String, String>,
        missing_names: &rustc_hash::FxHashSet<String>,
    ) -> Vec<ImportCandidate> {
        let existing_specifiers = Self::collect_existing_import_specifiers(files);
        let mut out = Vec::new();
        let mut seen = rustc_hash::FxHashSet::default();
        for path in external_project_paths {
            if path == current_file_path || !path.contains("/node_modules/") {
                continue;
            }
            if !(path.ends_with(".d.ts")
                || path.ends_with(".ts")
                || path.ends_with(".mts")
                || path.ends_with(".cts"))
            {
                continue;
            }
            let Some(specifier) =
                Self::module_specifier_from_node_modules_path(path, &existing_specifiers)
            else {
                continue;
            };
            for missing_name in missing_names {
                if seen.insert((specifier.clone(), missing_name.clone())) {
                    out.push(ImportCandidate::named(
                        specifier.clone(),
                        missing_name.clone(),
                        missing_name.clone(),
                    ));
                }
            }
        }
        out
    }

    fn extract_module_specifier_from_import_line(line: &str) -> Option<String> {
        let line = line.trim();
        if !line.starts_with("import ") {
            return None;
        }
        if let Some(idx) = line.find(" from \"") {
            let rest = &line[idx + 7..];
            return rest.find('"').map(|end| rest[..end].to_string());
        }
        if let Some(idx) = line.find(" from '") {
            let rest = &line[idx + 7..];
            return rest.find('\'').map(|end| rest[..end].to_string());
        }
        if let Some(rest) = line.strip_prefix("import \"") {
            return rest.find('"').map(|end| rest[..end].to_string());
        }
        if let Some(rest) = line.strip_prefix("import '") {
            return rest.find('\'').map(|end| rest[..end].to_string());
        }
        None
    }

    pub(super) fn module_specifier_from_node_modules_path(
        path: &str,
        existing_specifiers: &rustc_hash::FxHashSet<String>,
    ) -> Option<String> {
        let node_modules_idx = path.rfind("/node_modules/")?;
        let mut tail = &path[node_modules_idx + "/node_modules/".len()..];
        if tail.starts_with(".pnpm/")
            && let Some(inner_idx) = tail.find("/node_modules/")
        {
            tail = &tail[inner_idx + "/node_modules/".len()..];
        }

        let mut parts = tail.split('/');
        let first = parts.next()?;
        let package = if first.starts_with('@') {
            let second = parts.next()?;
            format!("{first}/{second}")
        } else {
            first.to_string()
        };
        let rest = tail.strip_prefix(&package).unwrap_or_default();

        let mut normalized_rest = rest.to_string();
        for ext in [
            ".d.ts", ".ts", ".tsx", ".js", ".jsx", ".mts", ".cts", ".mjs", ".cjs",
        ] {
            if normalized_rest.ends_with(ext) {
                let new_len = normalized_rest.len().saturating_sub(ext.len());
                normalized_rest.truncate(new_len);
                break;
            }
        }
        if normalized_rest.ends_with("/index") {
            let new_len = normalized_rest.len().saturating_sub("/index".len());
            normalized_rest.truncate(new_len);
        }

        let candidate = if normalized_rest.is_empty() {
            package.clone()
        } else if normalized_rest == format!("/dist/{package}") {
            package.clone()
        } else {
            format!("{package}{normalized_rest}")
        };

        if existing_specifiers.contains(&candidate) {
            Some(candidate)
        } else if existing_specifiers.contains(&package) {
            Some(package)
        } else {
            Some(candidate)
        }
    }
}
