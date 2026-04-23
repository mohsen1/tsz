//! Import management for code actions.
//!
//! Handles import removal, merging, insertion, and usage classification.
//! Extracted from `code_action_provider.rs` to keep files under the 2000-line limit.

use crate::diagnostics::LspDiagnostic;
use crate::rename::TextEdit;
use crate::utils::find_node_at_offset;
use rustc_hash::FxHashMap;
use std::path::Path;
use tsz_common::comments::get_leading_comments_from_cache;
use tsz_common::position::{Position, Range};
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

use super::code_action_provider::{
    CodeAction, CodeActionKind, CodeActionProvider, ImportCandidate, ImportCandidateKind,
};
use crate::rename::WorkspaceEdit;

// =============================================================================
// Import-related helper types
// =============================================================================

#[derive(Clone, Debug)]
struct NamedImportSpec {
    specifier: NodeIndex,
    import_name: String,
    local_name: String,
    is_type_only: bool,
}

#[derive(Clone, Debug)]
pub(super) enum ImportRemoval {
    Default { name: String },
    Namespace { name: String },
    Named { specifier: NodeIndex, name: String },
}

impl ImportRemoval {
    fn name(&self) -> &str {
        match self {
            Self::Default { name } | Self::Namespace { name } | Self::Named { name, .. } => name,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ImportUsage {
    Type,
    Value,
}

#[derive(Clone, Debug)]
pub(super) enum MergeNamedImport {
    Edits(Vec<TextEdit>),
    AlreadyImported,
    NoMatch,
}

#[derive(Clone, Debug)]
pub(super) enum MergeDefaultImport {
    Edits(Vec<TextEdit>),
    AlreadyImported,
    NoMatch,
}

fn compare_import_specifier_local_names(a: &str, b: &str, ignore_case: bool) -> std::cmp::Ordering {
    if !ignore_case {
        return a.cmp(b);
    }

    let a_folded = a.to_ascii_lowercase();
    let b_folded = b.to_ascii_lowercase();
    let a_case_rank = if a.chars().next().is_some_and(|ch| ch.is_ascii_lowercase()) {
        0
    } else {
        1
    };
    let b_case_rank = if b.chars().next().is_some_and(|ch| ch.is_ascii_lowercase()) {
        0
    } else {
        1
    };

    a_folded
        .cmp(&b_folded)
        .then_with(|| a_case_rank.cmp(&b_case_rank))
        .then_with(|| a.cmp(b))
}

fn module_specifier_match_for_merge(existing: &str, candidate: &str) -> bool {
    if existing == candidate {
        return true;
    }
    if !existing.starts_with('.') || !candidate.starts_with('.') {
        return false;
    }

    let extension_candidates = [".js", ".jsx", ".mjs", ".cjs"];
    let existing_has_ext = Path::new(existing).extension().is_some();
    let candidate_has_ext = Path::new(candidate).extension().is_some();

    if !existing_has_ext {
        for ext in extension_candidates {
            if format!("{existing}{ext}") == candidate {
                return true;
            }
        }
    }

    if !candidate_has_ext {
        for ext in extension_candidates {
            if format!("{candidate}{ext}") == existing {
                return true;
            }
        }
    }

    false
}

// =============================================================================
// Import management methods on CodeActionProvider
// =============================================================================

impl<'a> CodeActionProvider<'a> {
    // -------------------------------------------------------------------------
    // Import removal
    // -------------------------------------------------------------------------

    pub(super) fn import_removal_target(
        &self,
        node_idx: NodeIndex,
    ) -> Option<(NodeIndex, ImportRemoval)> {
        let mut current = node_idx;
        while current.is_some() {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::IMPORT_SPECIFIER {
                let name = self.specifier_local_name(current)?;
                let import_decl = self.find_import_decl(current)?;
                return Some((
                    import_decl,
                    ImportRemoval::Named {
                        specifier: current,
                        name,
                    },
                ));
            }

            if node.kind == SyntaxKind::Identifier as u16 {
                let parent = self.arena.get_extended(current)?.parent;
                if parent.is_none() {
                    return None;
                }

                let parent_node = self.arena.get(parent)?;
                if parent_node.kind == syntax_kind_ext::IMPORT_CLAUSE {
                    let clause = self.arena.get_import_clause(parent_node)?;
                    if clause.name == current {
                        let name = self.arena.get_identifier_text(current)?.to_string();
                        let import_decl = self.find_import_decl(parent)?;
                        return Some((import_decl, ImportRemoval::Default { name }));
                    }
                    if clause.named_bindings == current {
                        let name = self.arena.get_identifier_text(current)?.to_string();
                        let import_decl = self.find_import_decl(parent)?;
                        return Some((import_decl, ImportRemoval::Namespace { name }));
                    }
                }
            }

            current = self.arena.get_extended(current)?.parent;
        }

        None
    }

    fn find_import_decl(&self, start: NodeIndex) -> Option<NodeIndex> {
        let mut current = start;
        while current.is_some() {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::IMPORT_DECLARATION {
                return Some(current);
            }
            current = self.arena.get_extended(current)?.parent;
        }
        None
    }

    fn specifier_local_name(&self, spec_idx: NodeIndex) -> Option<String> {
        let spec_node = self.arena.get(spec_idx)?;
        let spec = self.arena.get_specifier(spec_node)?;
        let local_ident = if spec.name.is_some() {
            spec.name
        } else {
            spec.property_name
        };
        self.arena
            .get_identifier_text(local_ident)
            .map(std::string::ToString::to_string)
    }

    pub(super) fn build_import_removal_edit(
        &self,
        import_decl: NodeIndex,
        removal: ImportRemoval,
    ) -> Option<(TextEdit, String)> {
        let import_node = self.arena.get(import_decl)?;
        let import_data = self.arena.get_import_decl(import_node)?;
        if import_data.import_clause.is_none() {
            return None;
        }

        let clause_node = self.arena.get(import_data.import_clause)?;
        let clause = self.arena.get_import_clause(clause_node)?;

        let mut default_name = if clause.name.is_some() {
            self.arena
                .get_identifier_text(clause.name)
                .map(std::string::ToString::to_string)
        } else {
            None
        };

        let mut namespace_name = None;
        let mut named_specs = Vec::new();

        if clause.named_bindings.is_some() {
            let bindings_node = self.arena.get(clause.named_bindings)?;
            if bindings_node.kind == SyntaxKind::Identifier as u16 {
                namespace_name = self
                    .arena
                    .get_identifier_text(clause.named_bindings)
                    .map(std::string::ToString::to_string);
            } else if let Some(named) = self.arena.get_named_imports(bindings_node) {
                for &spec_idx in &named.elements.nodes {
                    let spec_node = self.arena.get(spec_idx)?;
                    let spec = self.arena.get_specifier(spec_node)?;
                    let import_ident = if spec.property_name.is_some() {
                        spec.property_name
                    } else {
                        spec.name
                    };
                    let local_ident = if spec.name.is_some() {
                        spec.name
                    } else {
                        spec.property_name
                    };
                    let import_name = self.arena.get_identifier_text(import_ident)?.to_string();
                    let local_name = self.arena.get_identifier_text(local_ident)?.to_string();
                    named_specs.push(NamedImportSpec {
                        specifier: spec_idx,
                        import_name,
                        local_name,
                        is_type_only: spec.is_type_only,
                    });
                }
            }
        }

        let removed_name = removal.name().to_string();
        match removal {
            ImportRemoval::Default { .. } => default_name = None,
            ImportRemoval::Namespace { .. } => namespace_name = None,
            ImportRemoval::Named { specifier, .. } => {
                named_specs.retain(|spec| spec.specifier != specifier);
            }
        }

        let has_named = !named_specs.is_empty();
        let has_namespace = namespace_name.is_some();
        let has_default = default_name.is_some();

        let (range, trailing) = self.import_decl_range(import_node);
        if !has_named && !has_namespace && !has_default {
            let edit = TextEdit {
                range,
                new_text: String::new(),
            };
            let title = format!("Remove unused import '{removed_name}'");
            return Some((edit, title));
        }

        let mut parts = Vec::new();
        if let Some(default_name) = default_name {
            parts.push(default_name);
        }
        if let Some(namespace_name) = namespace_name {
            parts.push(format!("* as {namespace_name}"));
        }
        if has_named {
            let mut items = Vec::new();
            for spec in named_specs {
                let mut item = String::new();
                if spec.is_type_only && !clause.is_type_only {
                    item.push_str("type ");
                }
                if spec.import_name == spec.local_name {
                    item.push_str(&spec.import_name);
                } else {
                    item.push_str(&format!("{} as {}", spec.import_name, spec.local_name));
                }
                items.push(item);
            }
            parts.push(format!("{{ {} }}", items.join(", ")));
        }

        let module_node = self.arena.get(import_data.module_specifier)?;
        let module_text = self
            .source
            .get(module_node.pos as usize..module_node.end as usize)?
            .to_string();

        let mut new_text = String::new();
        new_text.push_str("import ");
        if clause.is_type_only {
            new_text.push_str("type ");
        }
        new_text.push_str(&parts.join(", "));
        new_text.push_str(" from ");
        new_text.push_str(&module_text);
        new_text.push(';');
        new_text.push_str(&trailing);

        let edit = TextEdit { range, new_text };
        let title = format!("Remove unused import '{removed_name}'");

        Some((edit, title))
    }

    pub(super) fn import_decl_range(
        &self,
        node: &tsz_parser::parser::node::Node,
    ) -> (Range, String) {
        let mut end = node.end;
        if let Some(import_decl) = self.arena.get_import_decl(node)
            && let Some(module_node) = self.arena.get(import_decl.module_specifier)
        {
            end = module_node.end;
            if let Some(rest) = self.source.get(end as usize..) {
                let mut offset = 0usize;
                for &byte in rest.as_bytes() {
                    if byte.is_ascii_whitespace() {
                        if byte == b'\n' || byte == b'\r' {
                            break;
                        }
                        offset += 1;
                        continue;
                    }
                    if byte == b';' {
                        end += (offset + 1) as u32;
                    }
                    break;
                }
            }
        }
        let mut trailing = String::new();
        if let Some(rest) = self.source.get(end as usize..) {
            if rest.starts_with("\r\n") {
                end += 2;
                trailing = "\r\n".to_string();
            } else if rest.starts_with('\n') {
                end += 1;
                trailing = "\n".to_string();
            } else if rest.starts_with('\r') {
                end += 1;
                trailing = "\r".to_string();
            }
        }

        let start_pos = self.line_map.offset_to_position(node.pos, self.source);
        let end_pos = self.line_map.offset_to_position(end, self.source);
        (Range::new(start_pos, end_pos), trailing)
    }

    // -------------------------------------------------------------------------
    // Import usage classification
    // -------------------------------------------------------------------------

    pub(super) fn diagnostic_identifier_usage(
        &self,
        diag: &LspDiagnostic,
    ) -> Option<(String, ImportUsage)> {
        if let Some(node_idx) = self.identifier_node_at_range(diag.range)
            && let Some(node) = self.arena.get(node_idx)
            && node.kind == SyntaxKind::Identifier as u16
        {
            let name = self.arena.get_identifier_text(node_idx)?.to_string();
            let usage = self.import_usage_for_node(node_idx);
            return Some((name, usage));
        }

        let name = self.missing_name_from_diag(diag)?;
        let usage = self
            .find_identifier_usage_by_name(&name)
            .unwrap_or(ImportUsage::Value);
        Some((name, usage))
    }

    fn identifier_node_at_range(&self, range: Range) -> Option<NodeIndex> {
        let start_offset = self.line_map.position_to_offset(range.start, self.source)?;
        let end_offset = self
            .line_map
            .position_to_offset(range.end, self.source)
            .unwrap_or(start_offset);

        let mut try_offset = |offset: u32| {
            let node_idx = find_node_at_offset(self.arena, offset);
            let node = self.arena.get(node_idx)?;
            (node.kind == SyntaxKind::Identifier as u16).then_some(node_idx)
        };

        try_offset(start_offset)
            .or_else(|| end_offset.checked_sub(1).and_then(&mut try_offset))
            .or_else(|| start_offset.checked_sub(1).and_then(&mut try_offset))
    }

    fn missing_name_from_diag(&self, diag: &LspDiagnostic) -> Option<String> {
        let message = diag.message.as_str();
        let start = message.find('\'')?;
        let rest = &message[start + 1..];
        let end = rest.find('\'')?;
        Some(rest[..end].to_string())
    }

    fn find_identifier_usage_by_name(&self, name: &str) -> Option<ImportUsage> {
        for (idx, node) in self.arena.nodes.iter().enumerate() {
            if node.kind != SyntaxKind::Identifier as u16 {
                continue;
            }
            let node_idx = NodeIndex(idx as u32);
            let Some(text) = self.arena.get_identifier_text(node_idx) else {
                continue;
            };
            if text == name {
                return Some(self.import_usage_for_node(node_idx));
            }
        }
        None
    }

    fn import_usage_for_node(&self, node_idx: NodeIndex) -> ImportUsage {
        let mut current = node_idx;
        while current.is_some() {
            let Some(extended) = self.arena.get_extended(current) else {
                break;
            };
            if extended.parent.is_none() {
                break;
            }
            let parent_idx = extended.parent;
            let Some(parent_node) = self.arena.get(parent_idx) else {
                break;
            };

            if parent_node.kind == syntax_kind_ext::TYPE_QUERY {
                return ImportUsage::Value;
            }

            if parent_node.kind == syntax_kind_ext::HERITAGE_CLAUSE
                && let Some(usage) = self.import_usage_for_heritage_clause(parent_idx)
            {
                return usage;
            }

            if parent_node.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS
                && let Some(usage) = self.import_usage_in_heritage(parent_idx)
            {
                return usage;
            }

            if parent_node.is_type_node() {
                return ImportUsage::Type;
            }

            current = parent_idx;
        }

        ImportUsage::Value
    }

    fn import_usage_in_heritage(&self, expr_idx: NodeIndex) -> Option<ImportUsage> {
        let parent_idx = self.arena.get_extended(expr_idx)?.parent;
        self.import_usage_for_heritage_clause(parent_idx)
    }

    fn import_usage_for_heritage_clause(&self, clause_idx: NodeIndex) -> Option<ImportUsage> {
        if clause_idx.is_none() {
            return None;
        }
        let clause_node = self.arena.get(clause_idx)?;
        if clause_node.kind != syntax_kind_ext::HERITAGE_CLAUSE {
            return None;
        }

        let heritage = self.arena.get_heritage_clause(clause_node)?;
        let container_idx = self.arena.get_extended(clause_idx)?.parent;
        if container_idx.is_none() {
            return None;
        }
        let container_node = self.arena.get(container_idx)?;

        if heritage.token == SyntaxKind::ExtendsKeyword as u16 {
            if container_node.is_class_like() {
                return Some(ImportUsage::Value);
            }
            return Some(ImportUsage::Type);
        }

        if heritage.token == SyntaxKind::ImplementsKeyword as u16 {
            return Some(ImportUsage::Type);
        }

        None
    }

    // -------------------------------------------------------------------------
    // Import edit building and merging
    // -------------------------------------------------------------------------

    pub(super) fn build_import_edit(
        &self,
        root: NodeIndex,
        candidate: &ImportCandidate,
    ) -> Option<Vec<TextEdit>> {
        match &candidate.kind {
            ImportCandidateKind::Named { .. } => match self.try_merge_named_import(root, candidate)
            {
                MergeNamedImport::Edits(edits) => return Some(edits),
                MergeNamedImport::AlreadyImported => return None,
                MergeNamedImport::NoMatch => {}
            },
            ImportCandidateKind::Default => match self.try_merge_default_import(root, candidate) {
                MergeDefaultImport::Edits(edits) => return Some(edits),
                MergeDefaultImport::AlreadyImported => return None,
                MergeDefaultImport::NoMatch => {}
            },
            _ => {}
        }

        let (insert_pos, needs_newline) =
            self.import_insertion_point(root, &candidate.module_specifier)?;
        let insert_at_file_start = insert_pos.line == 0 && insert_pos.character == 0;
        let has_leading_import = insert_at_file_start && self.first_statement_is_import(root);
        // Match tsserver's behavior of picking the file's existing newline
        // style (preferring the first observed sequence), falling back to
        // CRLF when the source has no newlines (matches tsserver default).
        // An explicit override (from `format.newLineCharacter`) wins over
        // both the source scan and the CRLF default.
        let newline = if let Some(override_nl) = self.new_line_override.as_deref() {
            override_nl
        } else if self.source.contains("\r\n") {
            "\r\n"
        } else if self.source.contains('\n') {
            "\n"
        } else {
            "\r\n"
        };
        let mut new_text = String::new();
        if needs_newline {
            new_text.push_str(newline);
        }

        new_text.push_str("import ");
        if candidate.is_type_only {
            new_text.push_str("type ");
        }

        match &candidate.kind {
            ImportCandidateKind::Named { export_name } => {
                if export_name == &candidate.local_name {
                    new_text.push_str(&format!("{{ {export_name} }}"));
                } else {
                    new_text.push_str(&format!(
                        "{{ {} as {} }}",
                        export_name, candidate.local_name
                    ));
                }
            }
            ImportCandidateKind::Default => {
                new_text.push_str(&candidate.local_name);
            }
            ImportCandidateKind::Namespace => {
                new_text.push_str(&format!("* as {}", candidate.local_name));
            }
        }

        new_text.push_str(" from \"");
        new_text.push_str(&candidate.module_specifier);
        new_text.push_str("\";");
        new_text.push_str(newline);
        if insert_at_file_start && !has_leading_import {
            new_text.push_str(newline);
        }

        Some(vec![TextEdit {
            range: Range::new(insert_pos, insert_pos),
            new_text,
        }])
    }

    /// Generate import text edits for an auto-import completion.
    ///
    /// This is a public wrapper around `build_import_edit` specifically for
    /// use by the completion system when suggesting auto-imports.
    ///
    pub fn build_auto_import_edit(
        &self,
        root: NodeIndex,
        candidate: &ImportCandidate,
    ) -> Option<Vec<TextEdit>> {
        self.build_import_edit(root, candidate)
    }

    /// Generate completion auto-import text edits with usage-aware `import type`
    /// inference at the active cursor position.
    pub fn build_auto_import_edit_for_completion(
        &self,
        root: NodeIndex,
        candidate: &ImportCandidate,
        position: Position,
    ) -> Option<Vec<TextEdit>> {
        let mut resolved = candidate.clone();
        let usage = self
            .line_map
            .position_to_offset(position, self.source)
            .and_then(|offset| {
                let node_idx = find_node_at_offset(self.arena, offset);
                if node_idx.is_none() {
                    return None;
                }
                let node = self.arena.get(node_idx)?;
                if node.kind != SyntaxKind::Identifier as u16 {
                    return None;
                }
                Some(self.import_usage_for_node(node_idx))
            })
            .or_else(|| self.find_identifier_usage_by_name(&candidate.local_name))
            .unwrap_or(ImportUsage::Value);

        if usage == ImportUsage::Value && candidate.is_type_only {
            return None;
        }
        resolved.is_type_only = usage == ImportUsage::Type;

        self.build_import_edit(root, &resolved)
    }

    fn try_merge_default_import(
        &self,
        root: NodeIndex,
        candidate: &ImportCandidate,
    ) -> MergeDefaultImport {
        let ImportCandidateKind::Default = &candidate.kind else {
            return MergeDefaultImport::NoMatch;
        };

        let Some(root_node) = self.arena.get(root) else {
            return MergeDefaultImport::NoMatch;
        };
        let Some(source_file) = self.arena.get_source_file(root_node) else {
            return MergeDefaultImport::NoMatch;
        };

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION {
                continue;
            }

            let Some(import_decl) = self.arena.get_import_decl(stmt_node) else {
                continue;
            };
            if import_decl.import_clause.is_none() {
                continue;
            }

            let Some(module_text) = self.arena.get_literal_text(import_decl.module_specifier)
            else {
                continue;
            };
            if !module_specifier_match_for_merge(module_text, &candidate.module_specifier) {
                continue;
            }

            let Some(clause_node) = self.arena.get(import_decl.import_clause) else {
                continue;
            };
            let Some(clause) = self.arena.get_import_clause(clause_node) else {
                continue;
            };
            if clause.is_type_only != candidate.is_type_only {
                continue;
            }

            if clause.name.is_some() {
                if let Some(name) = self.arena.get_identifier_text(clause.name)
                    && name == candidate.local_name
                {
                    return MergeDefaultImport::AlreadyImported;
                }
                continue;
            }

            if clause.named_bindings.is_none() {
                continue;
            }

            if let Some(edit) =
                self.build_default_import_insertion_edit(clause_node, clause, candidate)
            {
                return MergeDefaultImport::Edits(vec![edit]);
            }
        }

        MergeDefaultImport::NoMatch
    }

    fn try_merge_named_import(
        &self,
        root: NodeIndex,
        candidate: &ImportCandidate,
    ) -> MergeNamedImport {
        let ImportCandidateKind::Named { .. } = &candidate.kind else {
            return MergeNamedImport::NoMatch;
        };

        let Some(root_node) = self.arena.get(root) else {
            return MergeNamedImport::NoMatch;
        };
        let Some(source_file) = self.arena.get_source_file(root_node) else {
            return MergeNamedImport::NoMatch;
        };

        let mut default_target_type_only = None;
        let mut default_target_value = None;
        let mut fallback_value_named_edits: Option<Vec<TextEdit>> = None;
        let mut fallback_upgrade_type_only_named_edits: Option<Vec<TextEdit>> = None;

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION {
                continue;
            }

            let Some(import_decl) = self.arena.get_import_decl(stmt_node) else {
                continue;
            };
            if import_decl.import_clause.is_none() {
                continue;
            }

            let Some(module_text) = self.arena.get_literal_text(import_decl.module_specifier)
            else {
                continue;
            };
            if !module_specifier_match_for_merge(module_text, &candidate.module_specifier) {
                continue;
            }

            let Some(clause_node) = self.arena.get(import_decl.import_clause) else {
                continue;
            };
            let Some(clause) = self.arena.get_import_clause(clause_node) else {
                continue;
            };
            if clause.is_type_only && !candidate.is_type_only {
                if fallback_upgrade_type_only_named_edits.is_none()
                    && clause.named_bindings.is_some()
                    && let Some(bindings_node) = self.arena.get(clause.named_bindings)
                    && bindings_node.kind != SyntaxKind::Identifier as u16
                    && let Some(named) = self.arena.get_named_imports(bindings_node)
                    && let Some(edit) =
                        self.build_type_only_named_import_upgrade_edit(stmt_idx, named, candidate)
                {
                    fallback_upgrade_type_only_named_edits = Some(vec![edit]);
                }
                continue;
            }

            if clause.named_bindings.is_some() {
                let bindings_idx = clause.named_bindings;
                let Some(bindings_node) = self.arena.get(bindings_idx) else {
                    continue;
                };
                if bindings_node.kind == SyntaxKind::Identifier as u16 {
                    continue;
                }

                if let Some(named) = self.arena.get_named_imports(bindings_node) {
                    if self.named_imports_has_local_name(named, &candidate.local_name) {
                        return MergeNamedImport::AlreadyImported;
                    }
                    let Some(spec_text) =
                        self.named_import_spec_text(candidate, clause.is_type_only)
                    else {
                        return MergeNamedImport::NoMatch;
                    };
                    if let Some(edits) = self.build_named_import_insertion_edits(
                        bindings_idx,
                        named,
                        &candidate.local_name,
                        &spec_text,
                    ) {
                        if candidate.is_type_only && !clause.is_type_only {
                            if fallback_value_named_edits.is_none() {
                                fallback_value_named_edits = Some(edits);
                            }
                            continue;
                        }
                        return MergeNamedImport::Edits(edits);
                    }
                    return MergeNamedImport::NoMatch;
                }
            } else if clause.is_type_only {
                default_target_type_only = Some(stmt_idx);
            } else {
                default_target_value = Some(stmt_idx);
            }
        }

        let default_target = if candidate.is_type_only {
            default_target_type_only.or(default_target_value)
        } else {
            default_target_value.or(default_target_type_only)
        };
        if let Some(import_idx) = default_target
            && let Some(edit) = self.build_default_import_named_edit(import_idx, candidate)
        {
            return MergeNamedImport::Edits(vec![edit]);
        }
        if let Some(edits) = fallback_upgrade_type_only_named_edits {
            return MergeNamedImport::Edits(edits);
        }
        if let Some(edits) = fallback_value_named_edits {
            return MergeNamedImport::Edits(edits);
        }

        MergeNamedImport::NoMatch
    }

    fn named_imports_has_local_name(
        &self,
        named: &tsz_parser::parser::node::NamedImportsData,
        local_name: &str,
    ) -> bool {
        for &spec_idx in &named.elements.nodes {
            let Some(spec_node) = self.arena.get(spec_idx) else {
                continue;
            };
            let Some(spec) = self.arena.get_specifier(spec_node) else {
                continue;
            };
            let local_ident = if spec.name.is_some() {
                spec.name
            } else {
                spec.property_name
            };
            if let Some(name) = self.arena.get_identifier_text(local_ident)
                && name == local_name
            {
                return true;
            }
        }

        false
    }

    fn named_import_spec_text(
        &self,
        candidate: &ImportCandidate,
        clause_is_type_only: bool,
    ) -> Option<String> {
        let ImportCandidateKind::Named { export_name } = &candidate.kind else {
            return None;
        };

        let mut text = String::new();
        if candidate.is_type_only && !clause_is_type_only {
            text.push_str("type ");
        }
        if export_name == &candidate.local_name {
            text.push_str(export_name);
        } else {
            text.push_str(&format!("{} as {}", export_name, candidate.local_name));
        }

        Some(text)
    }

    fn build_named_import_insertion_edits(
        &self,
        named_idx: NodeIndex,
        named: &tsz_parser::parser::node::NamedImportsData,
        local_name: &str,
        spec_text: &str,
    ) -> Option<Vec<TextEdit>> {
        let named_node = self.arena.get(named_idx)?;
        let close_offset = self.find_closing_brace_offset(named_node)?;
        let open_pos = self
            .line_map
            .offset_to_position(named_node.pos, self.source);
        let close_pos = self.line_map.offset_to_position(close_offset, self.source);
        let is_single_line = open_pos.line == close_pos.line;
        let elements = &named.elements.nodes;

        if is_single_line {
            if let Some(insert_offset) = self.single_line_named_import_sorted_insert_offset(
                elements,
                local_name,
                self.organize_imports_ignore_case,
            ) {
                let insert_pos = self.line_map.offset_to_position(insert_offset, self.source);
                return Some(vec![TextEdit {
                    range: Range::new(insert_pos, insert_pos),
                    new_text: format!("{spec_text}, "),
                }]);
            }

            let mut insert_offset = close_offset;
            while insert_offset > named_node.pos {
                let idx = (insert_offset - 1) as usize;
                let ch = *self.source.as_bytes().get(idx)?;
                if !ch.is_ascii_whitespace() {
                    break;
                }
                insert_offset -= 1;
            }
            let had_trailing_ws = insert_offset != close_offset;
            let trailing_space = if had_trailing_ws { "" } else { " " };
            let prefix = if elements.is_empty() { " " } else { ", " };
            let new_text = format!("{prefix}{spec_text}{trailing_space}");
            let insert_pos = self.line_map.offset_to_position(insert_offset, self.source);
            return Some(vec![TextEdit {
                range: Range::new(insert_pos, insert_pos),
                new_text,
            }]);
        }

        let close_line_start = self.line_map.line_start(close_pos.line as usize)?;
        let close_indent = self.indent_at_offset(close_line_start);
        let spec_indent = if let Some(&first) = elements.first() {
            let first_node = self.arena.get(first)?;
            self.indent_at_offset(first_node.pos)
        } else {
            let indent_unit = self.indent_unit_from(&close_indent);
            format!("{close_indent}{indent_unit}")
        };

        if let Some(&last) = elements.last() {
            let last_node = self.arena.get(last)?;
            let last_spec = self.arena.get_specifier(last_node)?;
            let last_ident = if last_spec.name.is_some() {
                last_spec.name
            } else {
                last_spec.property_name
            };
            let last_end = self
                .arena
                .get(last_ident)
                .map_or(last_node.end, |node| node.end);
            let between = self.source.get(last_end as usize..close_offset as usize)?;
            let trimmed = between.trim_start();
            if trimmed.contains("//") || trimmed.contains("/*") {
                return None;
            }
            let had_trailing_comma = trimmed.starts_with(',');
            let mut edits = Vec::new();
            if !had_trailing_comma {
                let last_pos = self.line_map.offset_to_position(last_end, self.source);
                edits.push(TextEdit {
                    range: Range::new(last_pos, last_pos),
                    new_text: ",".to_string(),
                });
            }

            let mut line = String::new();
            line.push_str(&spec_indent);
            line.push_str(spec_text);
            if had_trailing_comma {
                line.push(',');
            }
            line.push('\n');

            let insert_pos = self
                .line_map
                .offset_to_position(close_line_start, self.source);
            edits.push(TextEdit {
                range: Range::new(insert_pos, insert_pos),
                new_text: line,
            });
            return Some(edits);
        }

        let mut line = String::new();
        line.push_str(&spec_indent);
        line.push_str(spec_text);
        line.push('\n');
        let insert_pos = self
            .line_map
            .offset_to_position(close_line_start, self.source);
        Some(vec![TextEdit {
            range: Range::new(insert_pos, insert_pos),
            new_text: line,
        }])
    }

    fn single_line_named_import_sorted_insert_offset(
        &self,
        elements: &[NodeIndex],
        local_name: &str,
        ignore_case: bool,
    ) -> Option<u32> {
        for &spec_idx in elements {
            let spec_node = self.arena.get(spec_idx)?;
            let spec = self.arena.get_specifier(spec_node)?;
            let local_ident = if spec.name.is_some() {
                spec.name
            } else {
                spec.property_name
            };

            let Some(existing_local_name) = self.arena.get_identifier_text(local_ident) else {
                continue;
            };
            if compare_import_specifier_local_names(local_name, existing_local_name, ignore_case)
                == std::cmp::Ordering::Less
            {
                return Some(spec_node.pos);
            }
        }

        None
    }

    fn build_default_import_named_edit(
        &self,
        import_idx: NodeIndex,
        candidate: &ImportCandidate,
    ) -> Option<TextEdit> {
        let ImportCandidateKind::Named { .. } = &candidate.kind else {
            return None;
        };
        let import_node = self.arena.get(import_idx)?;
        let import_data = self.arena.get_import_decl(import_node)?;
        if import_data.import_clause.is_none() {
            return None;
        }

        let clause_node = self.arena.get(import_data.import_clause)?;
        let clause = self.arena.get_import_clause(clause_node)?;
        if clause.name.is_none() || clause.named_bindings.is_some() {
            return None;
        }
        if clause.is_type_only && !candidate.is_type_only {
            return None;
        }

        let default_name = self.arena.get_identifier_text(clause.name)?.to_string();
        let spec_text = self.named_import_spec_text(candidate, clause.is_type_only)?;

        let module_node = self.arena.get(import_data.module_specifier)?;
        let module_text = self
            .source
            .get(module_node.pos as usize..module_node.end as usize)?
            .to_string();

        let (range, trailing) = self.import_decl_range(import_node);
        let mut new_text = String::new();
        new_text.push_str("import ");
        if clause.is_type_only {
            new_text.push_str("type ");
        }
        new_text.push_str(&default_name);
        new_text.push_str(", { ");
        new_text.push_str(&spec_text);
        new_text.push_str(" } from ");
        new_text.push_str(&module_text);
        new_text.push(';');
        new_text.push_str(&trailing);

        Some(TextEdit { range, new_text })
    }

    fn build_type_only_named_import_upgrade_edit(
        &self,
        import_idx: NodeIndex,
        named: &tsz_parser::parser::node::NamedImportsData,
        candidate: &ImportCandidate,
    ) -> Option<TextEdit> {
        let ImportCandidateKind::Named { export_name } = &candidate.kind else {
            return None;
        };

        let import_node = self.arena.get(import_idx)?;
        let import_data = self.arena.get_import_decl(import_node)?;
        let clause_node = self.arena.get(import_data.import_clause)?;
        let clause = self.arena.get_import_clause(clause_node)?;
        if !clause.is_type_only || clause.name.is_some() {
            return None;
        }

        let mut entries: Vec<(String, String)> = Vec::new();
        let mut found_existing = false;
        for &spec_idx in &named.elements.nodes {
            let spec_node = self.arena.get(spec_idx)?;
            let spec = self.arena.get_specifier(spec_node)?;
            let import_ident = if spec.property_name.is_some() {
                spec.property_name
            } else {
                spec.name
            };
            let local_ident = if spec.name.is_some() {
                spec.name
            } else {
                spec.property_name
            };
            let import_name = self.arena.get_identifier_text(import_ident)?;
            let local_name = self.arena.get_identifier_text(local_ident)?;

            let mut rendered = String::new();
            if local_name == candidate.local_name {
                found_existing = true;
            } else {
                rendered.push_str("type ");
            }
            if import_name == local_name {
                rendered.push_str(import_name);
            } else {
                rendered.push_str(&format!("{import_name} as {local_name}"));
            }
            entries.push((local_name.to_string(), rendered));
        }

        if !found_existing {
            let mut rendered = String::new();
            if export_name == &candidate.local_name {
                rendered.push_str(export_name);
            } else {
                rendered.push_str(&format!("{export_name} as {}", candidate.local_name));
            }
            entries.push((candidate.local_name.clone(), rendered));
        }

        entries.sort_by(|(left, _), (right, _)| {
            compare_import_specifier_local_names(left, right, self.organize_imports_ignore_case)
        });

        let module_node = self.arena.get(import_data.module_specifier)?;
        let module_text = self
            .source
            .get(module_node.pos as usize..module_node.end as usize)?
            .to_string();
        let (range, trailing) = self.import_decl_range(import_node);

        let mut new_text = String::new();
        new_text.push_str("import { ");
        for (idx, (_, rendered)) in entries.iter().enumerate() {
            if idx > 0 {
                new_text.push_str(", ");
            }
            new_text.push_str(rendered);
        }
        new_text.push_str(" } from ");
        new_text.push_str(&module_text);
        new_text.push(';');
        new_text.push_str(&trailing);

        Some(TextEdit { range, new_text })
    }

    fn build_default_import_insertion_edit(
        &self,
        clause_node: &tsz_parser::parser::node::Node,
        clause: &tsz_parser::parser::node::ImportClauseData,
        candidate: &ImportCandidate,
    ) -> Option<TextEdit> {
        let bindings_idx = clause.named_bindings;
        if bindings_idx.is_none() {
            return None;
        }
        let bindings_node = self.arena.get(bindings_idx)?;
        let insert_offset = if bindings_node.kind == SyntaxKind::Identifier as u16 {
            self.namespace_import_star_offset(clause_node.pos, bindings_node.pos)?
        } else {
            bindings_node.pos
        };
        let insert_pos = self.line_map.offset_to_position(insert_offset, self.source);

        let mut new_text = String::new();
        if insert_offset > 0 {
            let prev = *self.source.as_bytes().get((insert_offset - 1) as usize)?;
            if !prev.is_ascii_whitespace() {
                new_text.push(' ');
            }
        }
        new_text.push_str(&candidate.local_name);
        new_text.push_str(", ");

        Some(TextEdit {
            range: Range::new(insert_pos, insert_pos),
            new_text,
        })
    }

    fn namespace_import_star_offset(&self, start: u32, end: u32) -> Option<u32> {
        if end <= start {
            return None;
        }
        let bytes = self.source.as_bytes();
        let mut offset = end;
        while offset > start {
            offset -= 1;
            if *bytes.get(offset as usize)? == b'*' {
                return Some(offset);
            }
        }
        None
    }

    fn import_insertion_point(
        &self,
        root: NodeIndex,
        module_specifier: &str,
    ) -> Option<(Position, bool)> {
        let root_node = self.arena.get(root)?;
        let source_file = self.arena.get_source_file(root_node)?;

        let mut last_import = None;
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind == syntax_kind_ext::IMPORT_DECLARATION
                || stmt_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                if stmt_node.kind == syntax_kind_ext::IMPORT_DECLARATION
                    && let Some(import_decl) = self.arena.get_import_decl(stmt_node)
                    && let Some(existing_module) =
                        self.arena.get_literal_text(import_decl.module_specifier)
                    && module_specifier < existing_module
                {
                    let insert_pos = self.line_map.offset_to_position(stmt_node.pos, self.source);
                    return Some((insert_pos, false));
                }
                last_import = Some(stmt_idx);
            }
        }

        if let Some(last_import) = last_import {
            let import_node = self.arena.get(last_import)?;
            let (range, trailing) = self.import_decl_range(import_node);
            let needs_newline = trailing.is_empty();
            return Some((range.end, needs_newline));
        }

        Some((Position::new(0, 0), false))
    }

    fn first_statement_is_import(&self, root: NodeIndex) -> bool {
        let Some(root_node) = self.arena.get(root) else {
            return false;
        };
        let Some(source_file) = self.arena.get_source_file(root_node) else {
            return false;
        };
        let Some(&first_stmt_idx) = source_file.statements.nodes.first() else {
            return false;
        };
        self.arena.get(first_stmt_idx).is_some_and(|stmt| {
            stmt.kind == syntax_kind_ext::IMPORT_DECLARATION
                || stmt.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
        })
    }

    // -------------------------------------------------------------------------
    // Organize imports
    // -------------------------------------------------------------------------

    /// Organize imports: sort contiguous import blocks by module specifier.
    pub fn organize_imports(&self, root: NodeIndex) -> Option<CodeAction> {
        let root_node = self.arena.get(root)?;
        let source_file = self.arena.get_source_file(root_node)?;

        let mut edits = Vec::new();
        let statements = &source_file.statements.nodes;
        let mut i = 0;

        while i < statements.len() {
            let start_idx = i;
            while i < statements.len() && self.is_import_declaration(statements[i]) {
                i += 1;
            }
            let end_idx = i;

            if end_idx > start_idx + 1
                && let Some(edit) =
                    self.sort_imports_range(&statements[start_idx..end_idx], &source_file.comments)
            {
                edits.push(edit);
            }

            while i < statements.len() && !self.is_import_declaration(statements[i]) {
                i += 1;
            }
        }

        if edits.is_empty() {
            return None;
        }

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), edits);

        Some(CodeAction {
            title: "Organize Imports".to_string(),
            kind: CodeActionKind::SourceOrganizeImports,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
            data: Some(serde_json::json!({
                "fixName": "organizeImports"
            })),
        })
    }

    /// Resolve a deferred organize-imports action by computing the edit.
    ///
    /// This is used by `codeAction/resolve` when the action was returned
    /// with `data` but no `edit`.
    pub fn resolve_organize_imports(&self, root: NodeIndex) -> Option<WorkspaceEdit> {
        self.organize_imports(root).and_then(|action| action.edit)
    }

    fn is_import_declaration(&self, node_idx: NodeIndex) -> bool {
        self.arena
            .get(node_idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::IMPORT_DECLARATION)
    }

    fn sort_imports_range(
        &self,
        import_nodes: &[NodeIndex],
        comments: &[tsz_common::comments::CommentRange],
    ) -> Option<TextEdit> {
        #[derive(Clone)]
        struct ImportInfo {
            start: u32,
            end: u32,
            text: String,
            module_specifier: String,
            is_side_effect: bool,
        }

        let mut imports = Vec::new();
        let mut block_start = u32::MAX;
        let mut block_end = 0u32;

        for &node_idx in import_nodes {
            let node = self.arena.get(node_idx)?;
            let leading = get_leading_comments_from_cache(comments, node.pos, self.source);
            let start = leading.first().map_or(node.pos, |c| c.pos);

            block_start = block_start.min(start);
            block_end = block_end.max(node.end);

            let import_decl = self.arena.get_import_decl(node)?;
            let is_side_effect = import_decl.import_clause.is_none();
            let specifier = self.get_module_specifier(node_idx).unwrap_or_default();
            let text = self
                .source
                .get(start as usize..node.end as usize)?
                .to_string();
            imports.push(ImportInfo {
                start,
                end: node.end,
                text,
                module_specifier: specifier,
                is_side_effect,
            });
        }

        if imports.is_empty() {
            return None;
        }

        let mut groups: Vec<Vec<ImportInfo>> = Vec::new();
        let mut separators: Vec<String> = Vec::new();
        let mut current = Vec::new();

        for idx in 0..imports.len() {
            let mut info = imports[idx].clone();
            if idx + 1 < imports.len() {
                let next_start = imports[idx + 1].start;
                let between = self
                    .source
                    .get(info.end as usize..next_start as usize)
                    .unwrap_or("");
                let has_blank_line = between.contains("\n\n")
                    || between.contains("\r\n\r\n")
                    || between.contains("\r\r");
                if has_blank_line {
                    current.push(info);
                    groups.push(std::mem::take(&mut current));
                    separators.push(between.to_string());
                    continue;
                }
                info.text.push_str(between);
                info.end = next_start;
            }
            current.push(info);
        }
        if !current.is_empty() {
            groups.push(current);
        }

        let mut new_text = String::new();
        for (group_idx, group) in groups.into_iter().enumerate() {
            let mut new_chunks = Vec::new();
            let mut pending = Vec::new();
            for info in group {
                if info.is_side_effect {
                    pending.sort_by(|a: &ImportInfo, b: &ImportInfo| {
                        a.module_specifier.cmp(&b.module_specifier)
                    });
                    for sorted in pending.drain(..) {
                        new_chunks.push(sorted.text);
                    }
                    new_chunks.push(info.text);
                } else {
                    pending.push(info);
                }
            }
            if !pending.is_empty() {
                pending.sort_by(|a, b| a.module_specifier.cmp(&b.module_specifier));
                for sorted in pending {
                    new_chunks.push(sorted.text);
                }
            }

            if group_idx > 0 {
                if let Some(sep) = separators.get(group_idx - 1) {
                    new_text.push_str(sep);
                } else {
                    new_text.push('\n');
                }
            }

            if !new_chunks.is_empty() {
                if !new_text.is_empty() && !new_text.ends_with('\n') {
                    new_text.push('\n');
                }
                for chunk in new_chunks {
                    new_text.push_str(&chunk);
                    if !chunk.ends_with('\n') && !chunk.ends_with('\r') {
                        new_text.push('\n');
                    }
                }
            }
        }

        let original = self.source.get(block_start as usize..block_end as usize)?;
        if original == new_text {
            return None;
        }

        let start_pos = self.line_map.offset_to_position(block_start, self.source);
        let end_pos = self.line_map.offset_to_position(block_end, self.source);

        Some(TextEdit {
            range: Range::new(start_pos, end_pos),
            new_text,
        })
    }

    fn get_module_specifier(&self, import_idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(import_idx)?;
        let import_decl = self.arena.get_import_decl(node)?;
        let spec_idx = import_decl.module_specifier;
        let text = self.arena.get_literal_text(spec_idx)?;
        Some(text.to_string())
    }

    // -------------------------------------------------------------------------
    // Missing import quick fixes
    // -------------------------------------------------------------------------

    pub(super) fn missing_import_quickfixes(
        &self,
        root: NodeIndex,
        diag: &LspDiagnostic,
        candidates: &[ImportCandidate],
    ) -> Vec<CodeAction> {
        let code = match diag.code {
            Some(code) => code,
            None => return Vec::new(),
        };
        if code != tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME
            && code != tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAMESPACE
        {
            return Vec::new();
        }

        let Some((missing_name, usage)) = self.diagnostic_identifier_usage(diag) else {
            return Vec::new();
        };

        let mut actions = Vec::new();
        for candidate in candidates {
            if candidate.local_name != missing_name {
                continue;
            }
            if usage == ImportUsage::Value && candidate.is_type_only {
                continue;
            }

            let mut resolved = candidate.clone();
            // Use `import type` when the identifier is only used in a type position
            // (type annotations, implements clauses, etc.). For value usage, use a
            // regular import so the symbol is available at runtime.
            resolved.is_type_only = usage == ImportUsage::Type;

            let Some(edits) = self.build_import_edit(root, &resolved) else {
                continue;
            };

            let mut changes = FxHashMap::default();
            changes.insert(self.file_name.clone(), edits);

            let title = format!(
                "Import '{}' from '{}'",
                candidate.local_name, candidate.module_specifier
            );
            actions.push(CodeAction {
                title,
                kind: CodeActionKind::QuickFix,
                edit: Some(WorkspaceEdit { changes }),
                is_preferred: false,
                data: Some(serde_json::json!({
                    "fixName": "import",
                    "fixId": "fixMissingImport",
                    "fixAllDescription": "Add all missing imports"
                })),
            });
        }

        actions
    }
}
