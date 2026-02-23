//! Navigation, definition, and reference handlers for tsz-server.

use std::path::{Path, PathBuf};

use super::{Server, TsServerRequest, TsServerResponse};
use tsz::binder::SymbolId;
use tsz::lsp::definition::GoToDefinition;
use tsz::lsp::highlighting::DocumentHighlightProvider;
use tsz::lsp::hover::HoverProvider;
use tsz::lsp::implementation::GoToImplementationProvider;
use tsz::lsp::position::LineMap;
use tsz::lsp::project::Project;
use tsz::lsp::references::FindReferences;
use tsz::lsp::rename::RenameProvider;
use tsz::lsp::symbols::document_symbols::DocumentSymbolProvider;
use tsz::parser::node::NodeAccess;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeInterner;

/// Bundled context for a parsed file, reducing parameter count in helpers.
struct ParsedFileContext<'a> {
    arena: &'a tsz::parser::node::NodeArena,
    binder: &'a tsz::binder::BinderState,
    line_map: &'a LineMap,
    root: tsz::parser::NodeIndex,
    source_text: &'a str,
    file: &'a str,
}

impl Server {
    fn build_project_for_file(&self, file_name: &str) -> Option<Project> {
        let mut files = self.open_files.clone();
        for project_files in self.external_project_files.values() {
            for path in project_files {
                if files.contains_key(path) {
                    continue;
                }
                if let Ok(content) = std::fs::read_to_string(path) {
                    files.insert(path.clone(), content);
                }
            }
        }
        if !files.contains_key(file_name)
            && let Ok(content) = std::fs::read_to_string(file_name)
        {
            files.insert(file_name.to_string(), content);
        }
        Self::add_project_config_files(&mut files, file_name);
        if files.is_empty() {
            return None;
        }

        let mut project = Project::new();
        project.set_allow_importing_ts_extensions(self.allow_importing_ts_extensions);
        project.set_auto_imports_allowed_without_tsconfig(
            self.auto_imports_allowed_for_inferred_projects,
        );
        project.set_import_module_specifier_ending(
            self.completion_import_module_specifier_ending.clone(),
        );
        project.set_import_module_specifier_preference(
            self.import_module_specifier_preference.clone(),
        );
        project
            .set_auto_import_file_exclude_patterns(self.auto_import_file_exclude_patterns.clone());
        project.set_auto_import_specifier_exclude_regexes(
            self.auto_import_specifier_exclude_regexes.clone(),
        );
        for (path, text) in files {
            project.set_file(path, text);
        }
        Some(project)
    }

    fn find_ancestor_of_kind(
        arena: &tsz::parser::node::NodeArena,
        mut node_idx: tsz::parser::NodeIndex,
        kind: u16,
    ) -> tsz::parser::NodeIndex {
        while node_idx.is_some() {
            let Some(node) = arena.get(node_idx) else {
                break;
            };
            if node.kind == kind {
                return node_idx;
            }
            let Some(ext) = arena.get_extended(node_idx) else {
                break;
            };
            node_idx = ext.parent;
        }
        tsz::parser::NodeIndex::NONE
    }

    fn node_text_opt(source_text: &str, node: &tsz::parser::node::Node) -> Option<String> {
        if node.end <= node.pos || node.end as usize > source_text.len() {
            return None;
        }
        Some(source_text[node.pos as usize..node.end as usize].to_string())
    }

    fn unquote(text: &str) -> String {
        text.trim()
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| {
                text.trim()
                    .strip_prefix('\'')
                    .and_then(|s| s.strip_suffix('\''))
            })
            .unwrap_or(text.trim())
            .to_string()
    }

    fn resolve_module_to_file(&self, from_file: &str, module_specifier: &str) -> Option<String> {
        fn normalize_path(path: &Path) -> String {
            let mut out = PathBuf::new();
            for comp in path.components() {
                match comp {
                    std::path::Component::CurDir => {}
                    std::path::Component::ParentDir => {
                        out.pop();
                    }
                    other => out.push(other.as_os_str()),
                }
            }
            if out.as_os_str().is_empty() {
                ".".to_string()
            } else {
                out.to_string_lossy().to_string()
            }
        }

        let spec = module_specifier.trim();
        if !spec.starts_with('.') {
            return None;
        }
        let base_dir = Path::new(from_file).parent()?;
        let joined = base_dir.join(spec);
        let mut candidates = Vec::new();
        candidates.push(joined.clone());
        for ext in [".ts", ".tsx", ".d.ts", ".mts", ".cts", ".js", ".jsx"] {
            candidates.push(PathBuf::from(format!("{}{}", joined.display(), ext)));
        }
        for idx in [
            "index.ts",
            "index.tsx",
            "index.d.ts",
            "index.mts",
            "index.cts",
            "index.js",
            "index.jsx",
        ] {
            candidates.push(joined.join(idx));
        }
        for candidate in candidates {
            let candidate_str = normalize_path(&candidate);
            if self.open_files.contains_key(&candidate_str) || candidate.exists() {
                return Some(candidate_str);
            }
        }
        None
    }

    fn alias_query_target(
        arena: &tsz::parser::node::NodeArena,
        source_text: &str,
        offset: u32,
    ) -> Option<(String, String)> {
        let mut specifier_idx = tsz::parser::NodeIndex::NONE;
        let mut candidates = Vec::with_capacity(4);
        candidates.push(tsz::lsp::utils::find_node_at_offset(arena, offset));
        candidates.push(tsz::lsp::utils::find_node_at_or_before_offset(
            arena,
            offset,
            source_text,
        ));
        if offset > 0 {
            candidates.push(tsz::lsp::utils::find_node_at_offset(
                arena,
                offset.saturating_sub(1),
            ));
        }
        if (offset as usize) < source_text.len() {
            candidates.push(tsz::lsp::utils::find_node_at_offset(
                arena,
                offset.saturating_add(1),
            ));
        }
        for node_idx in candidates {
            if node_idx.is_none() {
                continue;
            }
            let import_spec = Self::find_ancestor_of_kind(
                arena,
                node_idx,
                tsz::parser::syntax_kind_ext::IMPORT_SPECIFIER,
            );
            if import_spec.is_some() {
                specifier_idx = import_spec;
                break;
            }
            let export_spec = Self::find_ancestor_of_kind(
                arena,
                node_idx,
                tsz::parser::syntax_kind_ext::EXPORT_SPECIFIER,
            );
            if export_spec.is_some() {
                specifier_idx = export_spec;
                break;
            }
        }
        if specifier_idx.is_none() {
            return Self::alias_query_target_textual(source_text, offset);
        }
        let specifier_node = arena.get(specifier_idx)?;
        let spec_data = arena.get_specifier(specifier_node)?;
        let alias_name = {
            let mut selected: Option<String> = None;
            for symbol_idx in [spec_data.property_name, spec_data.name] {
                let Some(symbol_node) = arena.get(symbol_idx) else {
                    continue;
                };
                if symbol_node.kind != SyntaxKind::StringLiteral as u16 {
                    continue;
                }
                let text = Self::unquote(&Self::node_text_opt(source_text, symbol_node)?);
                if offset >= symbol_node.pos && offset <= symbol_node.end {
                    selected = Some(text);
                    break;
                }
                if selected.is_none() {
                    selected = Some(text);
                }
            }
            selected?
        };
        let ext = arena.get_extended(specifier_idx)?;
        if ext.parent.is_none() {
            return None;
        }
        let import_decl = Self::find_ancestor_of_kind(
            arena,
            specifier_idx,
            tsz::parser::syntax_kind_ext::IMPORT_DECLARATION,
        );
        if import_decl.is_some()
            && let Some(import_node) = arena.get(import_decl)
            && let Some(import_data) = arena.get_import_decl(import_node)
            && let Some(module_node) = arena.get(import_data.module_specifier)
            && let Some(module_text) = Self::node_text_opt(source_text, module_node)
        {
            return Some((Self::unquote(&module_text), alias_name));
        }
        let export_decl = Self::find_ancestor_of_kind(
            arena,
            specifier_idx,
            tsz::parser::syntax_kind_ext::EXPORT_DECLARATION,
        );
        if export_decl.is_some()
            && let Some(export_node) = arena.get(export_decl)
            && let Some(export_data) = arena.get_export_decl(export_node)
            && export_data.module_specifier.is_some()
            && let Some(module_node) = arena.get(export_data.module_specifier)
            && let Some(module_text) = Self::node_text_opt(source_text, module_node)
        {
            return Some((Self::unquote(&module_text), alias_name));
        }
        Self::alias_query_target_textual(source_text, offset)
    }

    fn alias_query_target_textual(source_text: &str, offset: u32) -> Option<(String, String)> {
        let bytes = source_text.as_bytes();
        let mut line_start = offset as usize;
        while line_start > 0 && bytes.get(line_start.wrapping_sub(1)) != Some(&b'\n') {
            line_start -= 1;
        }
        let mut line_end = offset as usize;
        while line_end < bytes.len() && bytes.get(line_end) != Some(&b'\n') {
            line_end += 1;
        }
        if line_start >= line_end || line_end > source_text.len() {
            return None;
        }
        let line = &source_text[line_start..line_end];
        let line_trim = line.trim();
        if !(line_trim.starts_with("import ") || line_trim.starts_with("export ")) {
            return None;
        }
        let mut quoted = Vec::new();
        let mut i = 0usize;
        let line_bytes = line.as_bytes();
        while i < line_bytes.len() {
            let ch = line_bytes[i];
            if ch != b'"' && ch != b'\'' {
                i += 1;
                continue;
            }
            let quote = ch;
            let start = i;
            i += 1;
            while i < line_bytes.len() && line_bytes[i] != quote {
                i += 1;
            }
            if i >= line_bytes.len() {
                break;
            }
            let end = i;
            let text = &line[start + 1..end];
            quoted.push((start, end + 1, text.to_string()));
            i += 1;
        }
        if quoted.is_empty() {
            return None;
        }
        let line_offset = offset as usize;
        let rel_offset = line_offset.saturating_sub(line_start);
        let alias_name = quoted
            .iter()
            .find(|(start, end, _)| rel_offset >= *start && rel_offset <= *end)
            .or_else(|| quoted.first())
            .map(|(_, _, text)| text.clone())?;

        let module_specifier = line
            .rfind(" from ")
            .and_then(|from_idx| {
                let after = &line[from_idx + 6..];
                after
                    .trim()
                    .strip_prefix('"')
                    .and_then(|s| s.strip_suffix('"'))
                    .or_else(|| {
                        after
                            .trim()
                            .strip_prefix('\'')
                            .and_then(|s| s.strip_suffix('\''))
                    })
                    .map(std::string::ToString::to_string)
            })
            .unwrap_or_default();
        if module_specifier.is_empty() {
            return None;
        }
        Some((module_specifier, alias_name))
    }

    fn is_quoted_import_or_export_specifier_offset(
        arena: &tsz::parser::node::NodeArena,
        source_text: &str,
        offset: u32,
    ) -> bool {
        Self::quoted_specifier_literal_at_offset(arena, source_text, offset).is_some()
    }

    fn is_quoted_import_or_export_specifier_location(
        &self,
        loc: &tsz_common::position::Location,
    ) -> bool {
        let Some((arena, _binder, _root, source_text)) = self.parse_and_bind_file(&loc.file_path)
        else {
            return false;
        };
        let line_map = LineMap::build(&source_text);
        let Some(offset) = line_map.position_to_offset(loc.range.start, &source_text) else {
            return false;
        };
        if Self::quoted_specifier_inner_range_at_offset(&arena, &source_text, offset).is_some() {
            return true;
        }
        let mut candidates = Vec::with_capacity(2);
        candidates.push(tsz::lsp::utils::find_node_at_offset(&arena, offset));
        candidates.push(tsz::lsp::utils::find_node_at_or_before_offset(
            &arena,
            offset,
            &source_text,
        ));
        for node_idx in candidates {
            if node_idx.is_none() {
                continue;
            }
            let Some(node) = arena.get(node_idx) else {
                continue;
            };
            if node.kind != SyntaxKind::StringLiteral as u16 {
                continue;
            }
            let in_import = Self::find_ancestor_of_kind(
                &arena,
                node_idx,
                tsz::parser::syntax_kind_ext::IMPORT_SPECIFIER,
            )
            .is_some();
            let in_export = Self::find_ancestor_of_kind(
                &arena,
                node_idx,
                tsz::parser::syntax_kind_ext::EXPORT_SPECIFIER,
            )
            .is_some();
            if in_import || in_export {
                return true;
            }
        }
        false
    }

    fn quoted_specifier_inner_range_at_offset(
        arena: &tsz::parser::node::NodeArena,
        source_text: &str,
        offset: u32,
    ) -> Option<tsz_common::position::Range> {
        let mut candidates = Vec::with_capacity(4);
        candidates.push(tsz::lsp::utils::find_node_at_offset(arena, offset));
        if (offset as usize) < source_text.len() {
            candidates.push(tsz::lsp::utils::find_node_at_offset(
                arena,
                offset.saturating_add(1),
            ));
        }
        if offset > 0 {
            candidates.push(tsz::lsp::utils::find_node_at_offset(
                arena,
                offset.saturating_sub(1),
            ));
        }
        candidates.push(tsz::lsp::utils::find_node_at_or_before_offset(
            arena,
            offset,
            source_text,
        ));
        let line_map = LineMap::build(source_text);
        for node_idx in candidates {
            if node_idx.is_none() {
                continue;
            }
            let Some(node) = arena.get(node_idx) else {
                continue;
            };
            if node.kind != SyntaxKind::StringLiteral as u16 {
                continue;
            }
            if node.end <= node.pos.saturating_add(1) {
                continue;
            }
            let in_import = Self::find_ancestor_of_kind(
                arena,
                node_idx,
                tsz::parser::syntax_kind_ext::IMPORT_SPECIFIER,
            )
            .is_some();
            let in_export = Self::find_ancestor_of_kind(
                arena,
                node_idx,
                tsz::parser::syntax_kind_ext::EXPORT_SPECIFIER,
            )
            .is_some();
            if !(in_import || in_export) {
                continue;
            }
            let inner_start = node.pos.saturating_add(1);
            let inner_end = node.end.saturating_sub(1);
            if inner_end <= inner_start {
                continue;
            }
            return Some(tsz_common::position::Range::new(
                line_map.offset_to_position(inner_start, source_text),
                line_map.offset_to_position(inner_end, source_text),
            ));
        }
        None
    }

    fn quoted_specifier_literal_from_specifier(
        arena: &tsz::parser::node::NodeArena,
        specifier_idx: tsz::parser::NodeIndex,
        offset: u32,
    ) -> Option<String> {
        let specifier_node = arena.get(specifier_idx)?;
        let spec = arena.get_specifier(specifier_node)?;
        let mut fallback: Option<String> = None;
        for symbol_idx in [spec.property_name, spec.name] {
            let node = arena.get(symbol_idx)?;
            if node.kind != SyntaxKind::StringLiteral as u16 {
                continue;
            }
            let text = arena.get_literal_text(symbol_idx)?.to_string();
            if offset >= node.pos && offset <= node.end {
                return Some(text);
            }
            if fallback.is_none() {
                fallback = Some(text);
            }
        }
        fallback
    }

    fn quoted_specifier_literal_from_ancestor(
        arena: &tsz::parser::node::NodeArena,
        node_idx: tsz::parser::NodeIndex,
        offset: u32,
    ) -> Option<String> {
        let import_spec = Self::find_ancestor_of_kind(
            arena,
            node_idx,
            tsz::parser::syntax_kind_ext::IMPORT_SPECIFIER,
        );
        if import_spec.is_some()
            && let Some(text) =
                Self::quoted_specifier_literal_from_specifier(arena, import_spec, offset)
        {
            return Some(text);
        }
        let export_spec = Self::find_ancestor_of_kind(
            arena,
            node_idx,
            tsz::parser::syntax_kind_ext::EXPORT_SPECIFIER,
        );
        if export_spec.is_some() {
            return Self::quoted_specifier_literal_from_specifier(arena, export_spec, offset);
        }
        None
    }

    fn quoted_specifier_literal_at_offset(
        arena: &tsz::parser::node::NodeArena,
        source_text: &str,
        offset: u32,
    ) -> Option<String> {
        let mut candidates = Vec::with_capacity(4);
        candidates.push(tsz::lsp::utils::find_node_at_offset(arena, offset));
        if (offset as usize) < source_text.len() {
            candidates.push(tsz::lsp::utils::find_node_at_offset(
                arena,
                offset.saturating_add(1),
            ));
        }
        if offset > 0 {
            candidates.push(tsz::lsp::utils::find_node_at_offset(
                arena,
                offset.saturating_sub(1),
            ));
        }
        candidates.push(tsz::lsp::utils::find_node_at_or_before_offset(
            arena,
            offset,
            source_text,
        ));
        for node_idx in candidates {
            if node_idx.is_none() {
                continue;
            }
            if let Some(text) =
                Self::quoted_specifier_literal_from_ancestor(arena, node_idx, offset)
            {
                return Some(text);
            }
        }
        None
    }

    fn adjusted_quoted_specifier_offset(
        arena: &tsz::parser::node::NodeArena,
        source_text: &str,
        offset: u32,
    ) -> u32 {
        if Self::is_quoted_import_or_export_specifier_offset(arena, source_text, offset) {
            return offset;
        }
        let bytes = source_text.as_bytes();
        let len = bytes.len() as u32;
        let mut probe = offset;
        while probe < len {
            let b = bytes[probe as usize];
            if b == b'\n' || b == b'\r' {
                break;
            }
            if Self::is_quoted_import_or_export_specifier_offset(arena, source_text, probe) {
                return probe;
            }
            probe += 1;
        }
        let mut probe = offset.saturating_sub(1);
        loop {
            let b = bytes[probe as usize];
            if b == b'\n' || b == b'\r' {
                break;
            }
            if Self::is_quoted_import_or_export_specifier_offset(arena, source_text, probe) {
                return probe;
            }
            if probe == 0 {
                break;
            }
            probe -= 1;
        }
        offset
    }

    fn quoted_specifier_symbol_locations_for_names(
        &self,
        names: &rustc_hash::FxHashSet<String>,
    ) -> Vec<tsz_common::position::Location> {
        let mut out = Vec::new();
        for file_path in self.open_files.keys() {
            let Some((arena, _binder, root, source_text)) = self.parse_and_bind_file(file_path)
            else {
                continue;
            };
            let Some(source_file) = arena.get_source_file_at(root) else {
                continue;
            };
            let line_map = LineMap::build(&source_text);
            for &stmt_idx in &source_file.statements.nodes {
                let Some(stmt_node) = arena.get(stmt_idx) else {
                    continue;
                };
                if stmt_node.kind == tsz::parser::syntax_kind_ext::IMPORT_DECLARATION {
                    let Some(import) = arena.get_import_decl(stmt_node) else {
                        continue;
                    };
                    if import.import_clause.is_none() {
                        continue;
                    }
                    let Some(clause) = arena.get_import_clause_at(import.import_clause) else {
                        continue;
                    };
                    if clause.named_bindings.is_none() {
                        continue;
                    }
                    let Some(named) = arena.get_named_imports_at(clause.named_bindings) else {
                        continue;
                    };
                    for &spec_idx in &named.elements.nodes {
                        let Some(spec) = arena.get_specifier_at(spec_idx) else {
                            continue;
                        };
                        let symbol_idx = if spec.property_name.is_some() {
                            spec.property_name
                        } else {
                            spec.name
                        };
                        let Some(symbol_node) = arena.get(symbol_idx) else {
                            continue;
                        };
                        if symbol_node.kind != SyntaxKind::StringLiteral as u16 {
                            continue;
                        }
                        let Some(text) = arena.get_literal_text(symbol_idx) else {
                            continue;
                        };
                        if !names.contains(text) {
                            continue;
                        }
                        let inner_start = symbol_node.pos.saturating_add(1);
                        let inner_end = symbol_node.end.saturating_sub(1);
                        if inner_end <= inner_start {
                            continue;
                        }
                        out.push(tsz_common::position::Location::new(
                            file_path.clone(),
                            tsz_common::position::Range::new(
                                line_map.offset_to_position(inner_start, &source_text),
                                line_map.offset_to_position(inner_end, &source_text),
                            ),
                        ));
                    }
                } else if stmt_node.kind == tsz::parser::syntax_kind_ext::EXPORT_DECLARATION {
                    let Some(export) = arena.get_export_decl(stmt_node) else {
                        continue;
                    };
                    if export.export_clause.is_none() {
                        continue;
                    }
                    let Some(clause) = arena.get_named_imports_at(export.export_clause) else {
                        continue;
                    };
                    for &spec_idx in &clause.elements.nodes {
                        let Some(spec) = arena.get_specifier_at(spec_idx) else {
                            continue;
                        };
                        for symbol_idx in [spec.property_name, spec.name] {
                            let Some(symbol_node) = arena.get(symbol_idx) else {
                                continue;
                            };
                            if symbol_node.kind != SyntaxKind::StringLiteral as u16 {
                                continue;
                            }
                            let Some(text) = arena.get_literal_text(symbol_idx) else {
                                continue;
                            };
                            if !names.contains(text) {
                                continue;
                            }
                            let inner_start = symbol_node.pos.saturating_add(1);
                            let inner_end = symbol_node.end.saturating_sub(1);
                            if inner_end <= inner_start {
                                continue;
                            }
                            out.push(tsz_common::position::Location::new(
                                file_path.clone(),
                                tsz_common::position::Range::new(
                                    line_map.offset_to_position(inner_start, &source_text),
                                    line_map.offset_to_position(inner_end, &source_text),
                                ),
                            ));
                        }
                    }
                }
            }
        }
        out
    }

    fn quoted_specifier_alias_closure(&self, seed_name: &str) -> rustc_hash::FxHashSet<String> {
        let mut names: rustc_hash::FxHashSet<String> = rustc_hash::FxHashSet::default();
        names.insert(seed_name.to_string());
        let mut changed = true;
        while changed {
            changed = false;
            for file_path in self.open_files.keys() {
                let Some((arena, _binder, root, _source_text)) =
                    self.parse_and_bind_file(file_path)
                else {
                    continue;
                };
                let Some(source_file) = arena.get_source_file_at(root) else {
                    continue;
                };
                for &stmt_idx in &source_file.statements.nodes {
                    let Some(stmt_node) = arena.get(stmt_idx) else {
                        continue;
                    };
                    if stmt_node.kind != tsz::parser::syntax_kind_ext::EXPORT_DECLARATION {
                        continue;
                    }
                    let Some(export) = arena.get_export_decl(stmt_node) else {
                        continue;
                    };
                    if export.export_clause.is_none() {
                        continue;
                    }
                    let Some(clause) = arena.get_named_imports_at(export.export_clause) else {
                        continue;
                    };
                    for &spec_idx in &clause.elements.nodes {
                        let Some(spec) = arena.get_specifier_at(spec_idx) else {
                            continue;
                        };
                        let Some(left_node) = arena.get(spec.property_name) else {
                            continue;
                        };
                        let Some(right_node) = arena.get(spec.name) else {
                            continue;
                        };
                        if left_node.kind != SyntaxKind::StringLiteral as u16
                            || right_node.kind != SyntaxKind::StringLiteral as u16
                        {
                            continue;
                        }
                        let Some(left_text) = arena.get_literal_text(spec.property_name) else {
                            continue;
                        };
                        let Some(right_text) = arena.get_literal_text(spec.name) else {
                            continue;
                        };
                        if names.contains(left_text) && names.insert(right_text.to_string()) {
                            changed = true;
                        }
                        if names.contains(right_text) && names.insert(left_text.to_string()) {
                            changed = true;
                        }
                    }
                }
            }
        }
        names
    }

    fn quoted_alias_chain_references(
        &self,
        project: &mut Project,
        file: &str,
        arena: &tsz::parser::node::NodeArena,
        source_text: &str,
        query_offset: u32,
        query_position: tsz_common::position::Position,
        quoted_only: bool,
    ) -> Option<Vec<tsz_common::position::Location>> {
        if !Self::is_quoted_import_or_export_specifier_offset(arena, source_text, query_offset) {
            return None;
        }
        let mut merged_refs = Vec::new();
        if let Some(direct_refs) = project.find_references(file, query_position) {
            merged_refs.extend(direct_refs);
        }
        if let Some(canonical_loc) =
            self.canonical_definition_for_alias_position(file, arena, source_text, query_offset)
            && let Some(canonical_refs) =
                project.find_references(&canonical_loc.file_path, canonical_loc.range.start)
        {
            merged_refs.extend(canonical_refs);
        }
        if merged_refs.is_empty() {
            let seed_name =
                Self::quoted_specifier_literal_at_offset(arena, source_text, query_offset)?;
            let names = self.quoted_specifier_alias_closure(&seed_name);
            let mut only_textual = self.quoted_specifier_symbol_locations_for_names(&names);
            only_textual.sort_by(|a, b| {
                let file_cmp = a.file_path.cmp(&b.file_path);
                if file_cmp != std::cmp::Ordering::Equal {
                    return file_cmp;
                }
                let start_cmp = (a.range.start.line, a.range.start.character)
                    .cmp(&(b.range.start.line, b.range.start.character));
                if start_cmp != std::cmp::Ordering::Equal {
                    return start_cmp;
                }
                (a.range.end.line, a.range.end.character)
                    .cmp(&(b.range.end.line, b.range.end.character))
            });
            only_textual.dedup_by(|a, b| a.file_path == b.file_path && a.range == b.range);
            if quoted_only {
                return (!only_textual.is_empty()).then_some(only_textual);
            }
            return (!only_textual.is_empty()).then_some(only_textual);
        }
        if let Some(seed_name) =
            Self::quoted_specifier_literal_at_offset(arena, source_text, query_offset)
        {
            let names = self.quoted_specifier_alias_closure(&seed_name);
            merged_refs.extend(self.quoted_specifier_symbol_locations_for_names(&names));
        }
        let normalized: Vec<_> = merged_refs
            .into_iter()
            .map(|mut loc| {
                if let Some((loc_arena, _binder, _root, loc_source)) =
                    self.parse_and_bind_file(&loc.file_path)
                {
                    let loc_line_map = LineMap::build(&loc_source);
                    if let Some(start_off) =
                        loc_line_map.position_to_offset(loc.range.start, &loc_source)
                        && let Some(inner_range) = Self::quoted_specifier_inner_range_at_offset(
                            &loc_arena,
                            &loc_source,
                            start_off,
                        )
                    {
                        loc.range = inner_range;
                    }
                }
                loc
            })
            .collect();
        merged_refs = normalized;
        merged_refs.sort_by(|a, b| {
            let file_cmp = a.file_path.cmp(&b.file_path);
            if file_cmp != std::cmp::Ordering::Equal {
                return file_cmp;
            }
            let start_cmp = (a.range.start.line, a.range.start.character)
                .cmp(&(b.range.start.line, b.range.start.character));
            if start_cmp != std::cmp::Ordering::Equal {
                return start_cmp;
            }
            (a.range.end.line, a.range.end.character)
                .cmp(&(b.range.end.line, b.range.end.character))
        });
        merged_refs.dedup_by(|a, b| a.file_path == b.file_path && a.range == b.range);
        if quoted_only {
            let filtered: Vec<_> = merged_refs
                .into_iter()
                .filter(|loc| self.is_quoted_import_or_export_specifier_location(loc))
                .collect();
            if !filtered.is_empty() {
                return Some(filtered);
            }
            if let Some(seed_name) =
                Self::quoted_specifier_literal_at_offset(arena, source_text, query_offset)
            {
                let names = self.quoted_specifier_alias_closure(&seed_name);
                let mut only_textual = self.quoted_specifier_symbol_locations_for_names(&names);
                only_textual.sort_by(|a, b| {
                    let file_cmp = a.file_path.cmp(&b.file_path);
                    if file_cmp != std::cmp::Ordering::Equal {
                        return file_cmp;
                    }
                    let start_cmp = (a.range.start.line, a.range.start.character)
                        .cmp(&(b.range.start.line, b.range.start.character));
                    if start_cmp != std::cmp::Ordering::Equal {
                        return start_cmp;
                    }
                    (a.range.end.line, a.range.end.character)
                        .cmp(&(b.range.end.line, b.range.end.character))
                });
                only_textual.dedup_by(|a, b| a.file_path == b.file_path && a.range == b.range);
                return (!only_textual.is_empty()).then_some(only_textual);
            }
            return None;
        }
        (!merged_refs.is_empty()).then_some(merged_refs)
    }

    #[cfg(test)]
    pub(crate) fn debug_alias_query_target(
        &self,
        arena: &tsz::parser::node::NodeArena,
        source_text: &str,
        offset: u32,
    ) -> Option<(String, String)> {
        let _ = self;
        Self::alias_query_target(arena, source_text, offset)
    }

    fn resolve_export_alias_definition(
        &self,
        from_file: &str,
        module_specifier: &str,
        export_name: &str,
        depth: usize,
    ) -> Option<tsz_common::position::Location> {
        if depth > 8 {
            return None;
        }
        let target_file = self.resolve_module_to_file(from_file, module_specifier)?;
        let (_arena, _binder, _root, source_text) = self.parse_and_bind_file(&target_file)?;
        let line_map = LineMap::build(&source_text);

        let mut line_start = 0usize;
        for line in source_text.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with("export") || !trimmed.contains('{') || !trimmed.contains('}') {
                line_start += line.len() + 1;
                continue;
            }
            let Some(open_brace) = trimmed.find('{') else {
                line_start += line.len() + 1;
                continue;
            };
            let Some(close_brace) = trimmed[open_brace + 1..].find('}') else {
                line_start += line.len() + 1;
                continue;
            };
            let close_brace = open_brace + 1 + close_brace;
            let clause = &trimmed[open_brace + 1..close_brace];
            let module_spec = Self::extract_quoted_after(trimmed, " from ");

            for segment in clause.split(',') {
                let part = segment
                    .trim()
                    .strip_prefix("type ")
                    .unwrap_or(segment.trim());
                let Some((left_raw, right_raw)) = part.split_once(" as ") else {
                    continue;
                };
                let left = left_raw.trim();
                let right = right_raw.trim();
                if Self::unquote(right) != export_name {
                    continue;
                }

                let next_name = Self::unquote(left);
                if let Some(module_name) = module_spec.clone() {
                    return self.resolve_export_alias_definition(
                        &target_file,
                        &module_name,
                        &next_name,
                        depth + 1,
                    );
                }

                let token_pos_in_line = line.find(left)?;
                let token_start = (line_start + token_pos_in_line) as u32;
                let token_end = token_start + left.len() as u32;
                let range = tsz_common::position::Range::new(
                    line_map.offset_to_position(token_start, &source_text),
                    line_map.offset_to_position(token_end, &source_text),
                );
                return Some(tsz_common::position::Location::new(target_file, range));
            }

            line_start += line.len() + 1;
        }

        None
    }

    #[cfg(test)]
    pub(crate) fn debug_resolve_export_alias_definition(
        &self,
        from_file: &str,
        module_specifier: &str,
        export_name: &str,
    ) -> Option<tsz_common::position::Location> {
        self.resolve_export_alias_definition(from_file, module_specifier, export_name, 0)
    }

    pub(crate) fn canonical_definition_for_alias_position(
        &self,
        file: &str,
        arena: &tsz::parser::node::NodeArena,
        source_text: &str,
        offset: u32,
    ) -> Option<tsz_common::position::Location> {
        if let Some((module_specifier, alias_name)) =
            Self::alias_query_target(arena, source_text, offset)
        {
            return self.resolve_export_alias_definition(file, &module_specifier, &alias_name, 0);
        }

        // Fallback for local export aliases with quoted specifiers:
        //   export { foo as "__<alias>" }
        // Return the local-side token location so definition providers can resolve to the
        // canonical declaration (`foo` in the example above).
        let node_idx = tsz::lsp::utils::find_node_at_or_before_offset(arena, offset, source_text);
        let node = arena.get(node_idx)?;
        if node.kind != SyntaxKind::StringLiteral as u16 {
            return None;
        }
        let specifier_idx = Self::find_ancestor_of_kind(
            arena,
            node_idx,
            tsz::parser::syntax_kind_ext::EXPORT_SPECIFIER,
        );
        if specifier_idx.is_none() {
            return None;
        }
        let export_decl_idx = Self::find_ancestor_of_kind(
            arena,
            specifier_idx,
            tsz::parser::syntax_kind_ext::EXPORT_DECLARATION,
        );
        if export_decl_idx.is_none() {
            return None;
        }
        let export_decl_node = arena.get(export_decl_idx)?;
        let export_decl = arena.get_export_decl(export_decl_node)?;
        if export_decl.module_specifier.is_some() {
            return None;
        }
        let spec_node = arena.get(specifier_idx)?;
        let spec = arena.get_specifier(spec_node)?;
        let local_node_idx = if spec.property_name.is_some() {
            let prop = arena.get(spec.property_name)?;
            if prop.kind != SyntaxKind::StringLiteral as u16 {
                Some(spec.property_name)
            } else {
                None
            }
        } else {
            None
        }
        .or_else(|| {
            let name_node = arena.get(spec.name)?;
            if name_node.kind != SyntaxKind::StringLiteral as u16 {
                Some(spec.name)
            } else {
                None
            }
        })?;
        let local_node = arena.get(local_node_idx)?;
        let line_map = LineMap::build(source_text);
        let range = tsz_common::position::Range::new(
            line_map.offset_to_position(local_node.pos, source_text),
            line_map.offset_to_position(local_node.end, source_text),
        );
        Some(tsz_common::position::Location::new(file.to_string(), range))
    }

    fn definition_info_from_location(
        &self,
        loc: &tsz_common::position::Location,
    ) -> Option<serde_json::Value> {
        let (arena, binder, root, source_text) = self.parse_and_bind_file(&loc.file_path)?;
        let line_map = LineMap::build(&source_text);
        let provider = GoToDefinition::new(
            &arena,
            &binder,
            &line_map,
            loc.file_path.clone(),
            &source_text,
        );
        let mut probe_positions = Vec::with_capacity(4);
        probe_positions.push(loc.range.start);
        if loc.range.start != loc.range.end {
            probe_positions.push(loc.range.end);
            if loc.range.end.character > 0 {
                probe_positions.push(tsz_common::position::Position::new(
                    loc.range.end.line,
                    loc.range.end.character - 1,
                ));
            }
        }
        let mut infos = Vec::new();
        for probe in probe_positions {
            infos = provider
                .get_definition_info(root, probe)
                .unwrap_or_default();
            if !infos.is_empty() {
                break;
            }
        }
        let picked = infos
            .iter()
            .find(|info| info.location.range == loc.range)
            .or_else(|| {
                infos.iter().find(|info| {
                    let start = info.location.range.start;
                    let end = info.location.range.end;
                    (start.line < loc.range.start.line
                        || (start.line == loc.range.start.line
                            && start.character <= loc.range.start.character))
                        && (end.line > loc.range.start.line
                            || (end.line == loc.range.start.line
                                && end.character >= loc.range.start.character))
                })
            })
            .or_else(|| infos.first())?;
        Some(Self::definition_info_to_json(picked, &loc.file_path))
    }

    fn is_offset_inside_comment(source_text: &str, offset: u32) -> bool {
        let idx = offset as usize;
        if idx > source_text.len() {
            return false;
        }

        // Line comments.
        let line_start = source_text[..idx].rfind('\n').map_or(0, |i| i + 1);
        if let Some(line_comment) = source_text[line_start..idx].find("//") {
            let comment_start = line_start + line_comment;
            if comment_start <= idx {
                return true;
            }
        }

        // Block comments (including JSDoc).
        let last_open = source_text[..idx].rfind("/*");
        let last_close = source_text[..idx].rfind("*/");
        matches!((last_open, last_close), (Some(open), Some(close)) if open > close)
            || matches!((last_open, last_close), (Some(_), None))
    }

    fn extract_alias_module_name(display_string: &str) -> Option<String> {
        let prefix = "(alias) module \"";
        let rest = display_string.strip_prefix(prefix)?;
        let end = rest.find('"')?;
        Some(rest[..end].to_string())
    }

    fn extract_alias_name(display_string: &str) -> Option<String> {
        let import_line = display_string
            .lines()
            .find(|line| line.starts_with("import "))?;
        let rest = import_line.strip_prefix("import ")?;
        if let Some(eq_idx) = rest.find(" = ") {
            return Some(rest[..eq_idx].trim().to_string());
        }
        let end = rest
            .find(|c: char| c.is_whitespace() || c == ',' || c == '{' || c == ';')
            .unwrap_or(rest.len());
        if end == 0 {
            return None;
        }
        Some(rest[..end].trim().to_string())
    }

    fn extract_quoted_after(haystack: &str, token: &str) -> Option<String> {
        let idx = haystack.find(token)?;
        let after = &haystack[idx + token.len()..];
        for quote in ['"', '\''] {
            if let Some(start) = after.find(quote) {
                let rem = &after[start + 1..];
                if let Some(end) = rem.find(quote) {
                    return Some(rem[..end].to_string());
                }
            }
        }
        None
    }

    fn extract_module_name_from_source_for_alias(
        source_text: &str,
        alias_name: &str,
    ) -> Option<String> {
        for line in source_text.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with("import ") || !trimmed.contains(alias_name) {
                continue;
            }
            if let Some(module_name) = Self::extract_quoted_after(trimmed, "require(") {
                return Some(module_name);
            }
            if let Some(module_name) = Self::extract_quoted_after(trimmed, " from ") {
                return Some(module_name);
            }
        }
        None
    }

    fn find_namespace_alias_decl_offsets(
        source_text: &str,
        alias_name: &str,
    ) -> Option<(u32, u32, u32, u32)> {
        let needle = format!("import * as {alias_name}");
        let stmt_start = source_text.find(&needle)?;
        let alias_rel = needle.find(alias_name)?;
        let alias_start = stmt_start + alias_rel;
        let alias_end = alias_start + alias_name.len();
        let context_start = source_text[..stmt_start].rfind('\n').map_or(0, |i| i + 1);
        let context_end = source_text[stmt_start..]
            .find('\n')
            .map_or(source_text.len(), |i| stmt_start + i);
        Some((
            alias_start as u32,
            alias_end as u32,
            context_start as u32,
            context_end as u32,
        ))
    }

    fn find_ambient_module_offsets(
        source_text: &str,
        module_name: &str,
    ) -> Option<(u32, u32, u32, u32)> {
        for quote in ['"', '\''] {
            let needle = format!("declare module {quote}{module_name}{quote}");
            if let Some(stmt_start) = source_text.find(&needle) {
                let literal_start = stmt_start + "declare module ".len();
                let literal_end = literal_start + module_name.len() + 2;
                let context_start = source_text[..stmt_start].rfind('\n').map_or(0, |i| i + 1);
                let context_end = source_text[stmt_start..]
                    .find('\n')
                    .map_or(source_text.len(), |i| stmt_start + i);
                return Some((
                    literal_start as u32,
                    literal_end as u32,
                    context_start as u32,
                    context_end as u32,
                ));
            }
        }
        None
    }

    fn find_ambient_module_definition_info(
        &self,
        module_name: &str,
    ) -> Option<tsz::lsp::definition::DefinitionInfo> {
        for (file_path, source_text) in &self.open_files {
            let Some((name_start, name_end, context_start, context_end)) =
                Self::find_ambient_module_offsets(source_text, module_name)
            else {
                continue;
            };
            let line_map = LineMap::build(source_text);
            let name_range = tsz::lsp::position::Range::new(
                line_map.offset_to_position(name_start, source_text),
                line_map.offset_to_position(name_end, source_text),
            );
            let context_range = tsz::lsp::position::Range::new(
                line_map.offset_to_position(context_start, source_text),
                line_map.offset_to_position(context_end, source_text),
            );
            return Some(tsz::lsp::definition::DefinitionInfo {
                location: tsz_common::position::Location {
                    file_path: file_path.clone(),
                    range: name_range,
                },
                context_span: Some(context_range),
                name: format!("\"{module_name}\""),
                kind: "module".to_string(),
                container_name: String::new(),
                container_kind: String::new(),
                is_local: false,
                is_ambient: true,
            });
        }
        None
    }

    fn maybe_remap_alias_to_ambient_module(
        &self,
        ctx: &ParsedFileContext<'_>,
        position: tsz_common::position::Position,
        infos: &[tsz::lsp::definition::DefinitionInfo],
    ) -> Option<Vec<tsz::lsp::definition::DefinitionInfo>> {
        let interner = TypeInterner::new();
        let provider = HoverProvider::new(
            ctx.arena,
            ctx.binder,
            ctx.line_map,
            &interner,
            ctx.source_text,
            ctx.file.to_string(),
        );
        let mut type_cache = None;
        let hover = provider.get_hover(ctx.root, position, &mut type_cache);
        let mut alias_name = hover
            .as_ref()
            .and_then(|hover_info| Self::extract_alias_name(&hover_info.display_string));
        if alias_name.is_none() {
            alias_name = hover.as_ref().and_then(|hover_info| {
                let range = hover_info.range?;
                let start = ctx
                    .line_map
                    .position_to_offset(range.start, ctx.source_text)?;
                let end = ctx
                    .line_map
                    .position_to_offset(range.end, ctx.source_text)?;
                if start >= end || end as usize > ctx.source_text.len() {
                    return None;
                }
                Some(ctx.source_text[start as usize..end as usize].to_string())
            });
        }
        if alias_name.is_none() {
            alias_name = infos.first().map(|info| info.name.clone());
        }
        if alias_name
            .as_deref()
            .is_some_and(|name| name.chars().any(char::is_whitespace))
        {
            alias_name = None;
        }
        let alias_name = alias_name.or_else(|| infos.first().map(|info| info.name.clone()))?;
        let namespace_decl = Self::find_namespace_alias_decl_offsets(ctx.source_text, &alias_name);
        let offset = ctx.line_map.position_to_offset(position, ctx.source_text)?;
        let on_declaration = if let Some(first) = infos.first() {
            if first.kind != "alias" {
                return None;
            }
            match (
                ctx.line_map
                    .position_to_offset(first.location.range.start, ctx.source_text),
                ctx.line_map
                    .position_to_offset(first.location.range.end, ctx.source_text),
            ) {
                (Some(start), Some(end)) => offset >= start && offset <= end,
                _ => false,
            }
        } else if let Some((alias_start, alias_end, _, _)) = namespace_decl {
            offset >= alias_start && offset <= alias_end
        } else {
            false
        };

        // Namespace import usages should navigate to the namespace import declaration.
        if let Some((alias_start, alias_end, context_start, context_end)) = namespace_decl
            && !on_declaration
        {
            let alias_range = tsz::lsp::position::Range::new(
                ctx.line_map
                    .offset_to_position(alias_start, ctx.source_text),
                ctx.line_map.offset_to_position(alias_end, ctx.source_text),
            );
            let context_range = tsz::lsp::position::Range::new(
                ctx.line_map
                    .offset_to_position(context_start, ctx.source_text),
                ctx.line_map
                    .offset_to_position(context_end, ctx.source_text),
            );
            return Some(vec![tsz::lsp::definition::DefinitionInfo {
                location: tsz_common::position::Location {
                    file_path: ctx.file.to_string(),
                    range: alias_range,
                },
                context_span: Some(context_range),
                name: alias_name,
                kind: "alias".to_string(),
                container_name: String::new(),
                container_kind: String::new(),
                is_local: true,
                is_ambient: false,
            }]);
        }

        let module_name = hover
            .as_ref()
            .and_then(|hover_info| Self::extract_alias_module_name(&hover_info.display_string))
            .or_else(|| {
                Self::extract_module_name_from_source_for_alias(ctx.source_text, &alias_name)
            })?;

        self.find_ambient_module_definition_info(&module_name)
            .map(|info| vec![info])
    }

    pub(crate) fn handle_definition(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let raw_offset = line_map.position_to_offset(position, &source_text)?;
            let offset = Self::adjusted_quoted_specifier_offset(&arena, &source_text, raw_offset);
            let position = line_map.offset_to_position(offset, &source_text);
            if Self::is_offset_inside_comment(&source_text, offset) {
                return None;
            }
            if let Some(canonical_loc) =
                self.canonical_definition_for_alias_position(&file, &arena, &source_text, offset)
            {
                if let Some(def) = self.definition_info_from_location(&canonical_loc) {
                    return Some(serde_json::json!([def]));
                }
            }
            let provider =
                GoToDefinition::new(&arena, &binder, &line_map, file.clone(), &source_text);
            let mut infos = provider
                .get_definition_info(root, position)
                .unwrap_or_default();
            let file_ctx = ParsedFileContext {
                arena: &arena,
                binder: &binder,
                line_map: &line_map,
                root,
                source_text: &source_text,
                file: &file,
            };
            if let Some(remapped) =
                self.maybe_remap_alias_to_ambient_module(&file_ctx, position, &infos)
            {
                infos = remapped;
            }
            if infos.is_empty() {
                return None;
            }
            let body: Vec<serde_json::Value> = infos
                .iter()
                .map(|info| Self::definition_info_to_json(info, &file))
                .collect();
            Some(serde_json::json!(body))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_definition_and_bound_span(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let raw_offset = line_map.position_to_offset(position, &source_text)?;
            let offset = Self::adjusted_quoted_specifier_offset(&arena, &source_text, raw_offset);
            let position = line_map.offset_to_position(offset, &source_text);
            if Self::is_offset_inside_comment(&source_text, offset) {
                return None;
            }
            if let Some(canonical_loc) =
                self.canonical_definition_for_alias_position(&file, &arena, &source_text, offset)
            {
                if let Some(definition) = self.definition_info_from_location(&canonical_loc) {
                    let text_span = if Self::is_quoted_import_or_export_specifier_offset(
                        &arena,
                        &source_text,
                        offset,
                    ) {
                        let node_idx = tsz::lsp::utils::find_node_at_or_before_offset(
                            &arena,
                            offset,
                            &source_text,
                        );
                        if node_idx.is_some() {
                            if let Some(node) = arena.get(node_idx) {
                                serde_json::json!({
                                    "start": Self::lsp_to_tsserver_position(line_map.offset_to_position(node.pos, &source_text)),
                                    "end": Self::lsp_to_tsserver_position(line_map.offset_to_position(node.end, &source_text)),
                                })
                            } else {
                                serde_json::json!({
                                    "start": Self::lsp_to_tsserver_position(position),
                                    "end": Self::lsp_to_tsserver_position(position),
                                })
                            }
                        } else {
                            serde_json::json!({
                                "start": Self::lsp_to_tsserver_position(position),
                                "end": Self::lsp_to_tsserver_position(position),
                            })
                        }
                    } else {
                        serde_json::json!({
                            "start": Self::lsp_to_tsserver_position(position),
                            "end": Self::lsp_to_tsserver_position(position),
                        })
                    };
                    let text_span = serde_json::json!({
                        "start": text_span["start"].clone(),
                        "end": text_span["end"].clone(),
                    });
                    return Some(serde_json::json!({
                        "definitions": [definition],
                        "textSpan": text_span,
                    }));
                }
            }
            let provider =
                GoToDefinition::new(&arena, &binder, &line_map, file.clone(), &source_text);
            let mut infos = provider
                .get_definition_info(root, position)
                .unwrap_or_default();
            let file_ctx = ParsedFileContext {
                arena: &arena,
                binder: &binder,
                line_map: &line_map,
                root,
                source_text: &source_text,
                file: &file,
            };
            if let Some(remapped) =
                self.maybe_remap_alias_to_ambient_module(&file_ctx, position, &infos)
            {
                infos = remapped;
            }
            if infos.is_empty() {
                return None;
            }

            // Build definitions array with rich metadata
            let definitions: Vec<serde_json::Value> = infos
                .iter()
                .map(|info| Self::definition_info_to_json(info, &file))
                .collect();

            // Compute textSpan from hover range for symbol-accurate bound spans.
            let interner = TypeInterner::new();
            let hover_provider =
                HoverProvider::new(&arena, &binder, &line_map, &interner, &source_text, file);
            let mut type_cache = None;
            let hover_range = hover_provider
                .get_hover(root, position, &mut type_cache)
                .and_then(|info| info.range)
                .filter(|range| range.start != range.end);
            let symbol_range = hover_range.or_else(|| {
                let mut probe = line_map.position_to_offset(position, &source_text)?;
                let max = source_text.len() as u32;
                let mut remaining = 256u32;
                while probe < max && remaining > 0 {
                    let node_idx =
                        tsz::lsp::utils::find_node_at_or_before_offset(&arena, probe, &source_text);
                    if node_idx.is_some()
                        && tsz::lsp::utils::is_symbol_query_node(&arena, node_idx)
                        && let Some(node) = arena.get(node_idx)
                    {
                        let start = line_map.offset_to_position(node.pos, &source_text);
                        let end = line_map.offset_to_position(node.end, &source_text);
                        if start != end {
                            return Some(tsz::lsp::position::Range::new(start, end));
                        }
                    }

                    let ch = source_text.as_bytes()[probe as usize];
                    if ch == b'\n' || ch == b'\r' {
                        break;
                    }
                    probe += 1;
                    remaining -= 1;
                }
                None
            });
            let text_span = symbol_range
                .map(|range| {
                    serde_json::json!({
                        "start": Self::lsp_to_tsserver_position(range.start),
                        "end": Self::lsp_to_tsserver_position(range.end),
                    })
                })
                .unwrap_or_else(|| {
                    serde_json::json!({
                        "start": Self::lsp_to_tsserver_position(position),
                        "end": Self::lsp_to_tsserver_position(position),
                    })
                });

            Some(serde_json::json!({
                "definitions": definitions,
                "textSpan": text_span,
            }))
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({
                "definitions": [],
                "textSpan": {"start": {"line": 1, "offset": 1}, "end": {"line": 1, "offset": 1}}
            }))),
        )
    }

    pub(crate) fn handle_references(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let raw_query_offset = line_map.position_to_offset(position, &source_text)?;
            let query_offset =
                Self::adjusted_quoted_specifier_offset(&arena, &source_text, raw_query_offset);
            let position = line_map.offset_to_position(query_offset, &source_text);
            if let Some(mut project) = self.build_project_for_file(&file)
                && let Some(locs) = self.quoted_alias_chain_references(
                    &mut project,
                    &file,
                    &arena,
                    &source_text,
                    query_offset,
                    position,
                    true,
                )
            {
                let definition_locs = project.get_definition(&file, position).unwrap_or_default();
                let refs: Vec<serde_json::Value> = locs
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
                        let is_definition = definition_locs
                            .iter()
                            .any(|def| def.file_path == loc.file_path && def.range == loc.range);
                        Some(serde_json::json!({
                            "file": loc.file_path,
                            "start": Self::lsp_to_tsserver_position(loc.range.start),
                            "end": Self::lsp_to_tsserver_position(loc.range.end),
                            "lineText": line_text,
                            "isWriteAccess": false,
                            "isDefinition": is_definition,
                        }))
                    })
                    .collect();
                return Some(serde_json::json!({
                    "refs": refs,
                    "symbolName": "",
                }));
            }
            if let Some(mut project) = self.build_project_for_file(&file)
                && let Some(canonical_loc) = self.canonical_definition_for_alias_position(
                    &file,
                    &arena,
                    &source_text,
                    query_offset,
                )
                && let Some(locs) =
                    project.find_references(&canonical_loc.file_path, canonical_loc.range.start)
            {
                let restrict_to_quoted =
                    Self::quoted_specifier_literal_at_offset(&arena, &source_text, query_offset)
                        .is_some();
                let definition_locs = vec![canonical_loc];
                let refs: Vec<serde_json::Value> = locs
                    .iter()
                    .filter(|loc| {
                        !restrict_to_quoted
                            || self.is_quoted_import_or_export_specifier_location(loc)
                    })
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
                        let is_definition = definition_locs
                            .iter()
                            .any(|def| def.file_path == loc.file_path && def.range == loc.range);
                        Some(serde_json::json!({
                            "file": loc.file_path,
                            "start": Self::lsp_to_tsserver_position(loc.range.start),
                            "end": Self::lsp_to_tsserver_position(loc.range.end),
                            "lineText": line_text,
                            "isWriteAccess": false,
                            "isDefinition": is_definition,
                        }))
                    })
                    .collect();
                return Some(serde_json::json!({
                    "refs": refs,
                    "symbolName": "",
                }));
            }
            let provider = FindReferences::new(&arena, &binder, &line_map, file, &source_text);
            let (_symbol_id, ref_infos) = provider.find_references_with_symbol(root, position)?;

            // Try to get symbol name from the position
            let symbol_name = {
                let ref_offset = line_map.position_to_offset(position, &source_text)?;
                let node_idx = tsz::lsp::utils::find_node_at_offset(&arena, ref_offset);
                if node_idx.is_some() {
                    arena
                        .get_identifier_text(node_idx)
                        .map(std::string::ToString::to_string)
                } else {
                    None
                }
            }
            .unwrap_or_default();

            let refs: Vec<serde_json::Value> = ref_infos
                .iter()
                .map(|ref_info| {
                    serde_json::json!({
                        "file": ref_info.location.file_path,
                        "start": Self::lsp_to_tsserver_position(ref_info.location.range.start),
                        "end": Self::lsp_to_tsserver_position(ref_info.location.range.end),
                        "lineText": ref_info.line_text,
                        "isWriteAccess": ref_info.is_write_access,
                        "isDefinition": ref_info.is_definition,
                    })
                })
                .collect();
            Some(serde_json::json!({
                "refs": refs,
                "symbolName": symbol_name,
            }))
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({"refs": [], "symbolName": ""}))),
        )
    }

    pub(crate) fn handle_document_highlights(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let provider = DocumentHighlightProvider::new(&arena, &binder, &line_map, &source_text);
            let highlights = provider.get_document_highlights(root, position)?;

            // Group highlights by file (tsserver groups by file, each with highlightSpans)
            let highlight_spans: Vec<serde_json::Value> = highlights
                .iter()
                .map(|hl| {
                    let kind_str = match hl.kind {
                        Some(tsz::lsp::highlighting::DocumentHighlightKind::Read) => "reference",
                        Some(tsz::lsp::highlighting::DocumentHighlightKind::Write) => {
                            "writtenReference"
                        }
                        Some(tsz::lsp::highlighting::DocumentHighlightKind::Text) | None => "none",
                    };
                    serde_json::json!({
                        "start": Self::lsp_to_tsserver_position(hl.range.start),
                        "end": Self::lsp_to_tsserver_position(hl.range.end),
                        "kind": kind_str,
                    })
                })
                .collect();
            // All highlights are in the same file for now
            Some(serde_json::json!([{
                "file": file,
                "highlightSpans": highlight_spans,
            }]))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_rename(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let raw_query_offset = line_map.position_to_offset(position, &source_text)?;
            let query_offset =
                Self::adjusted_quoted_specifier_offset(&arena, &source_text, raw_query_offset);
            let position = line_map.offset_to_position(query_offset, &source_text);
            let provider =
                RenameProvider::new(&arena, &binder, &line_map, file.clone(), &source_text);

            // Use the rich prepare_rename_info to get display name, kind, etc.
            let info = provider.prepare_rename_info(root, position);
            if !info.can_rename {
                return Some(serde_json::json!({
                    "info": {
                        "canRename": false,
                        "localizedErrorMessage": info.localized_error_message.unwrap_or_else(|| "You cannot rename this element.".to_string())
                    },
                    "locs": []
                }));
            }

            // Compute trigger span length from the range
            let start_offset = line_map
                .position_to_offset(info.trigger_span.start, &source_text)
                .unwrap_or(0) as usize;
            let end_offset = line_map
                .position_to_offset(info.trigger_span.end, &source_text)
                .unwrap_or(0) as usize;
            let trigger_length = end_offset.saturating_sub(start_offset);

            if let Some(mut project) = self.build_project_for_file(&file)
                && let Some(locs) = self.quoted_alias_chain_references(
                    &mut project,
                    &file,
                    &arena,
                    &source_text,
                    query_offset,
                    position,
                    true,
                )
            {
                let mut grouped: rustc_hash::FxHashMap<String, Vec<serde_json::Value>> =
                    rustc_hash::FxHashMap::default();
                for loc in locs {
                    grouped
                        .entry(loc.file_path.clone())
                        .or_default()
                        .push(serde_json::json!({
                            "start": Self::lsp_to_tsserver_position(loc.range.start),
                            "end": Self::lsp_to_tsserver_position(loc.range.end),
                        }));
                }
                let locs_json: Vec<serde_json::Value> = grouped
                    .into_iter()
                    .map(|(file_name, file_locs)| {
                        serde_json::json!({
                            "file": file_name,
                            "locs": file_locs,
                        })
                    })
                    .collect();
                return Some(serde_json::json!({
                    "info": {
                        "canRename": true,
                        "displayName": info.display_name,
                        "fullDisplayName": info.full_display_name,
                        "kind": info.kind,
                        "kindModifiers": info.kind_modifiers,
                        "triggerSpan": {
                            "start": Self::lsp_to_tsserver_position(info.trigger_span.start),
                            "length": trigger_length
                        }
                    },
                    "locs": locs_json
                }));
            }

            if let Some(mut project) = self.build_project_for_file(&file)
                && let Some(canonical_loc) = self.canonical_definition_for_alias_position(
                    &file,
                    &arena,
                    &source_text,
                    query_offset,
                )
                && let Some(locs) =
                    project.find_references(&canonical_loc.file_path, canonical_loc.range.start)
            {
                let restrict_to_quoted =
                    Self::quoted_specifier_literal_at_offset(&arena, &source_text, query_offset)
                        .is_some();
                let mut grouped: rustc_hash::FxHashMap<String, Vec<serde_json::Value>> =
                    rustc_hash::FxHashMap::default();
                for loc in locs {
                    if restrict_to_quoted
                        && !self.is_quoted_import_or_export_specifier_location(&loc)
                    {
                        continue;
                    }
                    grouped
                        .entry(loc.file_path.clone())
                        .or_default()
                        .push(serde_json::json!({
                            "start": Self::lsp_to_tsserver_position(loc.range.start),
                            "end": Self::lsp_to_tsserver_position(loc.range.end),
                        }));
                }
                let locs_json: Vec<serde_json::Value> = grouped
                    .into_iter()
                    .map(|(file_name, file_locs)| {
                        serde_json::json!({
                            "file": file_name,
                            "locs": file_locs,
                        })
                    })
                    .collect();
                return Some(serde_json::json!({
                    "info": {
                        "canRename": true,
                        "displayName": info.display_name,
                        "fullDisplayName": info.full_display_name,
                        "kind": info.kind,
                        "kindModifiers": info.kind_modifiers,
                        "triggerSpan": {
                            "start": Self::lsp_to_tsserver_position(info.trigger_span.start),
                            "length": trigger_length
                        }
                    },
                    "locs": locs_json
                }));
            }

            // Get rename locations from references with symbol info
            let find_refs =
                FindReferences::new(&arena, &binder, &line_map, file.clone(), &source_text);
            let (symbol_id, ref_infos) = find_refs
                .find_references_with_symbol(root, position)
                .unwrap_or((SymbolId::NONE, Vec::new()));

            // Get definition info for context spans
            let def_provider =
                GoToDefinition::new(&arena, &binder, &line_map, file.clone(), &source_text);
            let def_infos = if symbol_id.is_some() {
                def_provider.definition_infos_from_symbol(symbol_id)
            } else {
                None
            };

            let file_locs: Vec<serde_json::Value> = ref_infos
                .iter()
                .map(|ref_info| {
                    let mut loc = serde_json::json!({
                        "start": Self::lsp_to_tsserver_position(ref_info.location.range.start),
                        "end": Self::lsp_to_tsserver_position(ref_info.location.range.end),
                    });
                    // Add contextSpan for definition locations
                    if ref_info.is_definition
                        && let Some(ref defs) = def_infos
                    {
                        for def in defs {
                            if def.location.range == ref_info.location.range
                                && let Some(ref ctx) = def.context_span
                            {
                                loc["contextStart"] = Self::lsp_to_tsserver_position(ctx.start);
                                loc["contextEnd"] = Self::lsp_to_tsserver_position(ctx.end);
                                break;
                            }
                        }
                    }
                    loc
                })
                .collect();
            Some(serde_json::json!({
                "info": {
                    "canRename": true,
                    "displayName": info.display_name,
                    "fullDisplayName": info.full_display_name,
                    "kind": info.kind,
                    "kindModifiers": info.kind_modifiers,
                    "triggerSpan": {
                        "start": Self::lsp_to_tsserver_position(info.trigger_span.start),
                        "length": trigger_length
                    }
                },
                "locs": [{
                    "file": file,
                    "locs": file_locs,
                }]
            }))
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({
                "info": {"canRename": false, "localizedErrorMessage": "Not yet implemented"},
                "locs": []
            }))),
        )
    }

    pub(crate) fn handle_references_full(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let raw_query_position = Self::tsserver_to_lsp_position(line, offset);
            let raw_query_offset = line_map.position_to_offset(raw_query_position, &source_text)?;
            let query_offset =
                Self::adjusted_quoted_specifier_offset(&arena, &source_text, raw_query_offset);
            let position = line_map.offset_to_position(query_offset, &source_text);

            if let Some(mut project) = self.build_project_for_file(&file)
                && let Some(locs) = self.quoted_alias_chain_references(
                    &mut project,
                    &file,
                    &arena,
                    &source_text,
                    query_offset,
                    position,
                    false,
                )
            {
                let canonical_loc = self.canonical_definition_for_alias_position(
                    &file,
                    &arena,
                    &source_text,
                    query_offset,
                );
                let definition = canonical_loc
                    .as_ref()
                    .and_then(|loc| self.definition_info_from_location(loc))
                    .unwrap_or_else(|| Self::build_fallback_definition(&file, "alias", ""));
                let cursor_offset = line_map
                    .position_to_offset(position, &source_text)
                    .unwrap_or(0);
                let references: Vec<serde_json::Value> = locs
                    .iter()
                    .map(|loc| {
                        let source = self
                            .open_files
                            .get(&loc.file_path)
                            .cloned()
                            .or_else(|| std::fs::read_to_string(&loc.file_path).ok())
                            .unwrap_or_default();
                        let lm = LineMap::build(&source);
                        let start = lm.position_to_offset(loc.range.start, &source).unwrap_or(0);
                        let end = lm
                            .position_to_offset(loc.range.end, &source)
                            .unwrap_or(start);
                        let is_definition = canonical_loc.as_ref().is_some_and(|def| {
                            loc.file_path == def.file_path
                                && loc.range == def.range
                                && cursor_offset >= start
                                && cursor_offset < end
                        });
                        serde_json::json!({
                            "fileName": loc.file_path,
                            "textSpan": {
                                "start": start,
                                "length": end.saturating_sub(start),
                            },
                            "isWriteAccess": false,
                            "isDefinition": is_definition,
                        })
                    })
                    .collect();
                return Some(serde_json::json!([{
                    "definition": definition,
                    "references": references,
                }]));
            }

            // Get references with the resolved symbol
            let ref_provider =
                FindReferences::new(&arena, &binder, &line_map, file.clone(), &source_text);
            let (symbol_id, ref_infos) =
                ref_provider.find_references_with_symbol(root, position)?;

            // Get definition metadata using GoToDefinition helpers
            let def_provider =
                GoToDefinition::new(&arena, &binder, &line_map, file.clone(), &source_text);
            let def_infos = def_provider.definition_infos_from_symbol(symbol_id);

            // Get symbol info for display
            let symbol = binder.symbols.get(symbol_id)?;
            let kind_str = def_provider.symbol_flags_to_kind_string(symbol.flags);
            let symbol_name = symbol.escaped_name.clone();

            // Use HoverProvider to get the display string with type info
            let interner = TypeInterner::new();
            let hover_provider = HoverProvider::new(
                &arena,
                &binder,
                &line_map,
                &interner,
                &source_text,
                file.clone(),
            );
            let mut type_cache = None;
            let hover_info = hover_provider.get_hover(root, position, &mut type_cache);
            let display_string = hover_info
                .as_ref()
                .map(|h| h.display_string.clone())
                .unwrap_or_default();

            // Build definition object using first definition info
            let definition = if let Some(ref defs) = def_infos {
                if let Some(first_def) = defs.first() {
                    let def_start =
                        line_map.position_to_offset(first_def.location.range.start, &source_text);
                    let def_end =
                        line_map.position_to_offset(first_def.location.range.end, &source_text);

                    // Use display_string from HoverProvider if available for proper type info
                    let name = if !display_string.is_empty() {
                        display_string.clone()
                    } else {
                        format!("{} {}", first_def.kind, first_def.name)
                    };
                    let display_parts = if !display_string.is_empty() {
                        Self::parse_display_string_to_parts(
                            &display_string,
                            &first_def.kind,
                            &first_def.name,
                        )
                    } else {
                        Self::build_simple_display_parts(&first_def.kind, &first_def.name)
                    };

                    let mut def_json = serde_json::json!({
                        "containerKind": "",
                        "containerName": "",
                        "kind": first_def.kind,
                        "name": name,
                        "displayParts": display_parts,
                        "fileName": file,
                        "textSpan": {
                            "start": def_start.unwrap_or(0),
                            "length": def_end.unwrap_or(0).saturating_sub(def_start.unwrap_or(0)),
                        },
                    });
                    if let Some(ref ctx) = first_def.context_span {
                        let ctx_start = line_map.position_to_offset(ctx.start, &source_text);
                        let ctx_end = line_map.position_to_offset(ctx.end, &source_text);
                        let ctx_start_off = ctx_start.unwrap_or(0);
                        let ctx_end_off = ctx_end.unwrap_or(0);
                        let def_start_off = def_start.unwrap_or(0);
                        let def_end_off = def_end.unwrap_or(0);
                        // Skip contextSpan when it matches textSpan (e.g., catch clause vars)
                        if ctx_start_off != def_start_off || ctx_end_off != def_end_off {
                            def_json["contextSpan"] = serde_json::json!({
                                "start": ctx_start_off,
                                "length": ctx_end_off.saturating_sub(ctx_start_off),
                            });
                        }
                    }
                    if first_def.kind == "alias"
                        && let Some((ctx_start_off, ctx_end_off)) =
                            Self::import_statement_context_span(
                                &source_text,
                                def_start.unwrap_or(0),
                            )
                    {
                        def_json["contextSpan"] = serde_json::json!({
                            "start": ctx_start_off,
                            "length": ctx_end_off.saturating_sub(ctx_start_off),
                        });
                    }
                    def_json
                } else {
                    Self::build_fallback_definition(&file, &kind_str, &symbol_name)
                }
            } else {
                Self::build_fallback_definition(&file, &kind_str, &symbol_name)
            };

            // Build references array with byte-offset textSpans
            // Compute cursor offset for isDefinition check - TypeScript only sets
            // isDefinition=true when the cursor is ON the definition reference
            let cursor_offset = line_map
                .position_to_offset(position, &source_text)
                .unwrap_or(0);

            let references: Vec<serde_json::Value> = ref_infos
                .iter()
                .map(|ref_info| {
                    let start =
                        line_map.position_to_offset(ref_info.location.range.start, &source_text);
                    let end =
                        line_map.position_to_offset(ref_info.location.range.end, &source_text);
                    let start_off = start.unwrap_or(0);
                    let end_off = end.unwrap_or(0);

                    // isDefinition is only true when: (1) the reference IS a definition,
                    // AND (2) the cursor is at that reference's position
                    let is_definition = ref_info.is_definition
                        && cursor_offset >= start_off
                        && cursor_offset < end_off;

                    let mut ref_json = serde_json::json!({
                        "fileName": ref_info.location.file_path,
                        "textSpan": {
                            "start": start_off,
                            "length": end_off.saturating_sub(start_off),
                        },
                        "isWriteAccess": ref_info.is_write_access,
                        "isDefinition": is_definition,
                    });

                    // Add contextSpan for definition references
                    // Skip when contextSpan matches textSpan (e.g., catch clause variables)
                    if ref_info.is_definition
                        && let Some(ref defs) = def_infos
                    {
                        for def in defs {
                            if def.location.range == ref_info.location.range
                                && let Some(ref ctx) = def.context_span
                            {
                                let ctx_start =
                                    line_map.position_to_offset(ctx.start, &source_text);
                                let ctx_end = line_map.position_to_offset(ctx.end, &source_text);
                                let ctx_start_off = ctx_start.unwrap_or(0);
                                let ctx_end_off = ctx_end.unwrap_or(0);
                                // Only add contextSpan if it differs from the textSpan
                                if ctx_start_off != start_off || ctx_end_off != end_off {
                                    ref_json["contextSpan"] = serde_json::json!({
                                        "start": ctx_start_off,
                                        "length": ctx_end_off.saturating_sub(ctx_start_off),
                                    });
                                }
                                break;
                            }
                        }
                        if symbol.flags & tsz::binder::symbol_flags::ALIAS != 0
                            && let Some((ctx_start_off, ctx_end_off)) =
                                Self::import_statement_context_span(&source_text, start_off)
                        {
                            ref_json["contextSpan"] = serde_json::json!({
                                "start": ctx_start_off,
                                "length": ctx_end_off.saturating_sub(ctx_start_off),
                            });
                        }
                    }
                    ref_json
                })
                .collect();

            // Return as ReferencedSymbol array (single entry for single-file)
            Some(serde_json::json!([{
                "definition": definition,
                "references": references,
            }]))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
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

    fn import_statement_context_span(source_text: &str, anchor_offset: u32) -> Option<(u32, u32)> {
        if source_text.is_empty() {
            return None;
        }
        let idx = (anchor_offset as usize).min(source_text.len().saturating_sub(1));
        let line_start = source_text[..idx].rfind('\n').map_or(0, |i| i + 1);
        let line_end = source_text[idx..]
            .find('\n')
            .map_or(source_text.len(), |i| idx + i);
        let line_text = source_text[line_start..line_end].trim_start();
        if !line_text.starts_with("import ") {
            return None;
        }
        Some((line_start as u32, line_end as u32))
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
            let (arena, _binder, root, source_text) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&source_text);
            let provider = DocumentSymbolProvider::new(&arena, &line_map, &source_text);
            let symbols = provider.get_document_symbols(root);

            fn symbol_to_navtree(
                sym: &tsz::lsp::symbols::document_symbols::DocumentSymbol,
            ) -> serde_json::Value {
                let kind = match sym.kind {
                    tsz::lsp::symbols::document_symbols::SymbolKind::File
                    | tsz::lsp::symbols::document_symbols::SymbolKind::Module
                    | tsz::lsp::symbols::document_symbols::SymbolKind::Namespace => "module",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Class => "class",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Method => "method",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Property
                    | tsz::lsp::symbols::document_symbols::SymbolKind::Field => "property",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Constructor => "constructor",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Enum => "enum",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Interface => "interface",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Function => "function",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Variable => "var",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Constant => "const",
                    tsz::lsp::symbols::document_symbols::SymbolKind::EnumMember => "enum member",
                    tsz::lsp::symbols::document_symbols::SymbolKind::TypeParameter => {
                        "type parameter"
                    }
                    tsz::lsp::symbols::document_symbols::SymbolKind::Struct => "type",
                    _ => "unknown",
                };
                let children: Vec<serde_json::Value> =
                    sym.children.iter().map(symbol_to_navtree).collect();
                serde_json::json!({
                    "text": sym.name,
                    "kind": kind,
                    "childItems": children,
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
                })
            }

            let child_items: Vec<serde_json::Value> =
                symbols.iter().map(symbol_to_navtree).collect();

            // Compute the end span based on source text length
            let total_lines = source_text.lines().count();
            let last_line_len = source_text.lines().last().map_or(0, str::len);
            Some(serde_json::json!({
                "text": "<global>",
                "kind": "script",
                "childItems": child_items,
                "spans": [{"start": {"line": 1, "offset": 1}, "end": {"line": total_lines, "offset": last_line_len + 1}}],
            }))
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({
                "text": "<global>",
                "kind": "script",
                "childItems": [],
                "spans": [{"start": {"line": 1, "offset": 1}, "end": {"line": 1, "offset": 1}}],
            }))),
        )
    }

    pub(crate) fn handle_navbar(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let (arena, _binder, root, source_text) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&source_text);
            let provider = DocumentSymbolProvider::new(&arena, &line_map, &source_text);
            let symbols = provider.get_document_symbols(root);

            fn symbol_to_navbar_item(
                sym: &tsz::lsp::symbols::document_symbols::DocumentSymbol,
                indent: usize,
                items: &mut Vec<serde_json::Value>,
            ) {
                let kind = match sym.kind {
                    tsz::lsp::symbols::document_symbols::SymbolKind::File
                    | tsz::lsp::symbols::document_symbols::SymbolKind::Module
                    | tsz::lsp::symbols::document_symbols::SymbolKind::Namespace => "module",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Class => "class",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Method => "method",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Property
                    | tsz::lsp::symbols::document_symbols::SymbolKind::Field => "property",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Constructor => "constructor",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Enum => "enum",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Interface => "interface",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Function => "function",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Variable => "var",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Constant => "const",
                    tsz::lsp::symbols::document_symbols::SymbolKind::EnumMember => "enum member",
                    tsz::lsp::symbols::document_symbols::SymbolKind::TypeParameter => {
                        "type parameter"
                    }
                    tsz::lsp::symbols::document_symbols::SymbolKind::Struct => "type",
                    _ => "unknown",
                };
                let child_items: Vec<serde_json::Value> = sym
                    .children
                    .iter()
                    .map(|c| {
                        serde_json::json!({
                            "text": c.name,
                            "kind": match c.kind {
                                tsz::lsp::symbols::document_symbols::SymbolKind::Function => "function",
                                tsz::lsp::symbols::document_symbols::SymbolKind::Class => "class",
                                tsz::lsp::symbols::document_symbols::SymbolKind::Method => "method",
                                tsz::lsp::symbols::document_symbols::SymbolKind::Property => "property",
                                tsz::lsp::symbols::document_symbols::SymbolKind::Variable => "var",
                                tsz::lsp::symbols::document_symbols::SymbolKind::Constant => "const",
                                tsz::lsp::symbols::document_symbols::SymbolKind::Enum => "enum",
                                tsz::lsp::symbols::document_symbols::SymbolKind::Interface => "interface",
                                tsz::lsp::symbols::document_symbols::SymbolKind::EnumMember => "enum member",
                                tsz::lsp::symbols::document_symbols::SymbolKind::Struct => "type",
                                _ => "unknown",
                            },
                        })
                    })
                    .collect();
                items.push(serde_json::json!({
                    "text": sym.name,
                    "kind": kind,
                    "childItems": child_items,
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
                }));
                for child in &sym.children {
                    symbol_to_navbar_item(child, indent + 1, items);
                }
            }

            let mut items = Vec::new();
            // Root item
            let total_lines = source_text.lines().count();
            let last_line_len = source_text.lines().last().map_or(0, str::len);
            let child_items: Vec<serde_json::Value> = symbols
                .iter()
                .map(|sym| {
                    serde_json::json!({
                        "text": sym.name,
                        "kind": match sym.kind {
                            tsz::lsp::symbols::document_symbols::SymbolKind::Function => "function",
                            tsz::lsp::symbols::document_symbols::SymbolKind::Class => "class",
                            tsz::lsp::symbols::document_symbols::SymbolKind::Method => "method",
                            tsz::lsp::symbols::document_symbols::SymbolKind::Property => "property",
                            tsz::lsp::symbols::document_symbols::SymbolKind::Variable => "var",
                            tsz::lsp::symbols::document_symbols::SymbolKind::Constant => "const",
                            tsz::lsp::symbols::document_symbols::SymbolKind::Enum => "enum",
                            tsz::lsp::symbols::document_symbols::SymbolKind::Interface => "interface",
                            tsz::lsp::symbols::document_symbols::SymbolKind::EnumMember => "enum member",
                            _ => "unknown",
                        },
                    })
                })
                .collect();
            items.push(serde_json::json!({
                "text": "<global>",
                "kind": "script",
                "childItems": child_items,
                "indent": 0,
                "spans": [{"start": {"line": 1, "offset": 1}, "end": {"line": total_lines, "offset": last_line_len + 1}}],
            }));
            // Flatten children
            for sym in &symbols {
                symbol_to_navbar_item(sym, 1, &mut items);
            }
            Some(serde_json::json!(items))
        })();
        self.stub_response(
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
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
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
                let kind = match sym.kind {
                    tsz::lsp::symbols::document_symbols::SymbolKind::Module => "module",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Class => "class",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Method => "method",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Property
                    | tsz::lsp::symbols::document_symbols::SymbolKind::Field => "property",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Constructor => "constructor",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Enum => "enum",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Interface => "interface",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Function => "function",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Variable => "var",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Constant => "const",
                    tsz::lsp::symbols::document_symbols::SymbolKind::EnumMember => "enum member",
                    tsz::lsp::symbols::document_symbols::SymbolKind::TypeParameter => {
                        "type parameter"
                    }
                    _ => "unknown",
                };
                let match_kind = if name_lower == *search_lower {
                    "exact"
                } else if name_lower.starts_with(search_lower) {
                    "prefix"
                } else {
                    "substring"
                };
                result.push(serde_json::json!({
                    "name": sym.name,
                    "kind": kind,
                    "kindModifiers": "",
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
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let provider =
                GoToImplementationProvider::new(&arena, &binder, &line_map, file, &source_text);
            let locations = provider.get_implementations(root, position)?;
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
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_file_references(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(
            seq,
            request,
            Some(serde_json::json!({"refs": [], "symbolName": ""})),
        )
    }
}
