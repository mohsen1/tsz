//! Module alias and quoted specifier resolution helpers for tsz-server.
//!
//! Extracted from `handlers_info.rs` — these functions handle the resolution of
//! quoted import/export specifiers (e.g., `import { "foo" as bar }`) and
//! module alias chains used by definition, references, and rename handlers.

use std::path::{Path, PathBuf};

use super::Server;
use tsz::lsp::definition::GoToDefinition;
use tsz::lsp::hover::HoverProvider;
use tsz::lsp::position::LineMap;
use tsz::parser::node::NodeAccess;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeInterner;

use super::handlers_info::ParsedFileContext;

impl Server {
    pub(super) fn unquote(text: &str) -> String {
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

    pub(super) fn resolve_module_to_file(
        &self,
        from_file: &str,
        module_specifier: &str,
    ) -> Option<String> {
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

    pub(super) fn alias_query_target(
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

    pub(super) fn is_quoted_import_or_export_specifier_offset(
        arena: &tsz::parser::node::NodeArena,
        source_text: &str,
        offset: u32,
    ) -> bool {
        Self::quoted_specifier_literal_at_offset(arena, source_text, offset).is_some()
    }

    pub(super) fn is_quoted_import_or_export_specifier_location(
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

    pub(super) fn quoted_specifier_inner_range_at_offset(
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

    pub(super) fn quoted_specifier_literal_at_offset(
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

    pub(super) fn adjusted_quoted_specifier_offset(
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

    pub(super) fn quoted_specifier_symbol_locations_for_names(
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

    pub(super) fn quoted_specifier_alias_closure(
        &self,
        seed_name: &str,
    ) -> rustc_hash::FxHashSet<String> {
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

    pub(super) fn quoted_alias_chain_references(
        &self,
        project: &mut tsz::lsp::project::Project,
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

    pub(super) fn resolve_export_alias_definition(
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

    pub(super) fn canonical_definition_for_alias_position(
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

    pub(super) fn definition_info_from_location(
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

    pub(super) fn is_offset_inside_comment(source_text: &str, offset: u32) -> bool {
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

    pub(super) fn extract_alias_module_name(display_string: &str) -> Option<String> {
        let prefix = "(alias) module \"";
        let rest = display_string.strip_prefix(prefix)?;
        let end = rest.find('"')?;
        Some(rest[..end].to_string())
    }

    pub(super) fn extract_alias_name(display_string: &str) -> Option<String> {
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

    pub(super) fn extract_quoted_after(haystack: &str, token: &str) -> Option<String> {
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

    pub(super) fn extract_module_name_from_source_for_alias(
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

    pub(super) fn find_namespace_alias_decl_offsets(
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

    pub(super) fn find_ambient_module_definition_info(
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

    pub(super) fn maybe_remap_alias_to_ambient_module(
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

    pub(super) fn import_statement_context_span(
        source_text: &str,
        anchor_offset: u32,
    ) -> Option<(u32, u32)> {
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
}
