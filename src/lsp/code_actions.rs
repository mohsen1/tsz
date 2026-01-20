//! Code Actions for the LSP.
//!
//! Provides quick fixes and refactorings to improve code quality and fix errors.
//!
//! Architecture:
//! - The Checker identifies problems (Diagnostics)
//! - The CodeActionProvider identifies solutions (TextEdits)
//!
//! Current features:
//! - Extract Variable (selection-based refactoring)
//! - Organize Imports (sort-only)
//! - Remove Unused Import (diagnostic-based quick fix)
//! - Add Missing Property (diagnostic-based quick fix, local declarations)
//! - Add Missing Import (diagnostic-based quick fix, project-aware)
//!
//! Future features:
//! - Remove Unused Declarations (diagnostic-based quick fix)

use crate::binder::{ScopeId, SymbolId, symbol_flags};
use crate::comments::get_leading_comments_from_cache;
use crate::lsp::diagnostics::LspDiagnostic;
use crate::lsp::position::{LineMap, Position, Range};
use crate::lsp::rename::{TextEdit, WorkspaceEdit};
use crate::lsp::utils::find_node_at_offset;
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::parser::node::{NodeAccess, Node, NodeArena};
use crate::scanner::SyntaxKind;
use crate::binder::BinderState;
use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};

// =============================================================================
// Code Action Types
// =============================================================================

/// Kind of code action (matches LSP spec).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CodeActionKind {
    /// Quick fix for an error or warning.
    #[serde(rename = "quickfix")]
    QuickFix,
    /// Generic refactoring action.
    #[serde(rename = "refactor")]
    Refactor,
    /// Extract to variable/constant/function.
    #[serde(rename = "refactor.extract")]
    RefactorExtract,
    /// Inline variable/function.
    #[serde(rename = "refactor.inline")]
    RefactorInline,
    /// Organize imports.
    #[serde(rename = "source.organizeImports")]
    SourceOrganizeImports,
}

#[derive(Debug, Clone)]
pub enum ImportCandidateKind {
    Named { export_name: String },
    Default,
    Namespace,
}

#[derive(Debug, Clone)]
pub struct ImportCandidate {
    pub module_specifier: String,
    pub local_name: String,
    pub kind: ImportCandidateKind,
    pub is_type_only: bool,
}

impl ImportCandidate {
    pub fn named(module_specifier: String, export_name: String, local_name: String) -> Self {
        Self {
            module_specifier,
            local_name,
            kind: ImportCandidateKind::Named { export_name },
            is_type_only: false,
        }
    }

    pub fn default(module_specifier: String, local_name: String) -> Self {
        Self {
            module_specifier,
            local_name,
            kind: ImportCandidateKind::Default,
            is_type_only: false,
        }
    }

    pub fn namespace(module_specifier: String, local_name: String) -> Self {
        Self {
            module_specifier,
            local_name,
            kind: ImportCandidateKind::Namespace,
            is_type_only: false,
        }
    }
}

/// A code action represents a change that can be performed in code.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeAction {
    /// A short, human-readable title for this code action.
    pub title: String,
    /// The kind of the code action.
    pub kind: CodeActionKind,
    /// The workspace edit to apply.
    pub edit: Option<WorkspaceEdit>,
    /// Marks this as a preferred action (shown first in UI).
    pub is_preferred: bool,
}

/// Context passed when requesting code actions.
#[derive(Debug, Clone)]
pub struct CodeActionContext {
    /// Diagnostics at the requested position (for quick fixes).
    pub diagnostics: Vec<LspDiagnostic>,
    /// Only return actions of these kinds (client filter).
    pub only: Option<Vec<CodeActionKind>>,
    /// Candidate imports for missing import quick fixes.
    pub import_candidates: Vec<ImportCandidate>,
}

// =============================================================================
// Code Action Provider
// =============================================================================

/// Provides code actions for a given position/range in the source code.
pub struct CodeActionProvider<'a> {
    arena: &'a NodeArena,
    binder: &'a BinderState,
    line_map: &'a LineMap,
    file_name: String,
    source: &'a str,
}

impl<'a> CodeActionProvider<'a> {
    /// Create a new code action provider.
    pub fn new(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        line_map: &'a LineMap,
        file_name: String,
        source: &'a str,
    ) -> Self {
        Self {
            arena,
            binder,
            line_map,
            file_name,
            source,
        }
    }

    /// Provide code actions for a range in the source code.
    pub fn provide_code_actions(
        &self,
        root: NodeIndex,
        range: Range,
        context: CodeActionContext,
    ) -> Vec<CodeAction> {
        let mut actions = Vec::new();

        // Quick Fixes (diagnostic-based)
        let request_quickfix = context
            .only
            .as_ref()
            .is_none_or(|kinds| kinds.contains(&CodeActionKind::QuickFix));
        if request_quickfix {
            for diag in &context.diagnostics {
                if let Some(action) = self.unused_import_quickfix(diag) {
                    actions.push(action);
                }
                if let Some(action) = self.unused_declaration_quickfix(diag) {
                    actions.push(action);
                }
                if let Some(action) = self.missing_property_quickfix(diag) {
                    actions.push(action);
                }
                actions.extend(self.missing_import_quickfixes(
                    root,
                    diag,
                    &context.import_candidates,
                ));
            }
        }

        // Source Actions (file-level)
        let request_organize = context
            .only
            .as_ref()
            .is_none_or(|kinds| kinds.contains(&CodeActionKind::SourceOrganizeImports));
        if request_organize {
            if let Some(action) = self.organize_imports(root) {
                actions.push(action);
            }
        }

        // Refactorings (selection-based)
        // Only if the range is non-empty (user selected text)
        if range.start != range.end {
            if let Some(action) = self.extract_variable(root, range) {
                actions.push(action);
            }
        }

        actions
    }

    fn unused_import_quickfix(&self, diag: &LspDiagnostic) -> Option<CodeAction> {
        let code = diag.code?;
        if code != crate::checker::types::diagnostics::diagnostic_codes::UNUSED_IMPORT {
            return None;
        }

        let start_offset = self
            .line_map
            .position_to_offset(diag.range.start, self.source)?;
        let node_idx = find_node_at_offset(self.arena, start_offset);
        if node_idx.is_none() {
            return None;
        }

        let (import_decl, removal) = self.import_removal_target(node_idx)?;
        let (edit, title) = self.build_import_removal_edit(import_decl, removal)?;

        let mut changes = std::collections::HashMap::new();
        changes.insert(self.file_name.clone(), vec![edit]);

        Some(CodeAction {
            title,
            kind: CodeActionKind::QuickFix,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: true,
        })
    }

    fn unused_declaration_quickfix(&self, diag: &LspDiagnostic) -> Option<CodeAction> {
        let code = diag.code?;
        if code != crate::checker::types::diagnostics::diagnostic_codes::UNUSED_VARIABLE {
            return None;
        }

        let start_offset = self
            .line_map
            .position_to_offset(diag.range.start, self.source)?;
        let node_idx = find_node_at_offset(self.arena, start_offset);
        if node_idx.is_none() {
            return None;
        }

        // Find the parent declaration node
        let decl_node = self.find_declaration_node(node_idx)?;
        let (edit, name) = self.build_declaration_removal_edit(decl_node)?;

        let mut changes = std::collections::HashMap::new();
        changes.insert(self.file_name.clone(), vec![edit]);

        let title = format!("Remove unused declaration '{}'", name);

        Some(CodeAction {
            title,
            kind: CodeActionKind::QuickFix,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: true,
        })
    }

    /// Find the declaration node containing the given node (identifier).
    fn find_declaration_node(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = node_idx;
        for _ in 0..10 {
            // Traverse up to find the declaration
            let Some(node) = self.arena.get(current) else {
                break;
            };

            match node.kind {
                syntax_kind_ext::VARIABLE_DECLARATION
                | syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::INTERFACE_DECLARATION
                | syntax_kind_ext::TYPE_ALIAS_DECLARATION
                | syntax_kind_ext::ENUM_DECLARATION => {
                    return Some(current);
                }
                _ => {}
            }

            let Some(ext) = self.arena.get_extended(current) else {
                break;
            };
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }

        None
    }

    /// Build a text edit to remove a declaration node.
    fn build_declaration_removal_edit(&self, decl_idx: NodeIndex) -> Option<(TextEdit, String)> {
        let decl_node = self.arena.get(decl_idx)?;
        let (range, _trailing) = self.declaration_removal_range(decl_node);

        // Get the declaration name for the title
        let name = match decl_node.kind {
            syntax_kind_ext::VARIABLE_DECLARATION => {
                let var_decl = self.arena.get_variable_declaration(decl_node)?;
                self.arena.get_identifier_text(var_decl.name)?.to_string()
            }
            syntax_kind_ext::FUNCTION_DECLARATION => {
                let func = self.arena.get_function(decl_node)?;
                self.arena.get_identifier_text(func.name)?.to_string()
            }
            syntax_kind_ext::CLASS_DECLARATION => {
                let class = self.arena.get_class(decl_node)?;
                self.arena.get_identifier_text(class.name)?.to_string()
            }
            syntax_kind_ext::INTERFACE_DECLARATION => {
                let iface = self.arena.get_interface(decl_node)?;
                self.arena.get_identifier_text(iface.name)?.to_string()
            }
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                let alias = self.arena.get_type_alias(decl_node)?;
                self.arena.get_identifier_text(alias.name)?.to_string()
            }
            syntax_kind_ext::ENUM_DECLARATION => {
                let enum_decl = self.arena.get_enum(decl_node)?;
                self.arena.get_identifier_text(enum_decl.name)?.to_string()
            }
            _ => "declaration".to_string(),
        };

        let edit = TextEdit {
            range,
            new_text: String::new(),
        };

        Some((edit, name))
    }

    /// Get the range for removing a declaration, including handling for multi-line declarations.
    fn declaration_removal_range(
        &self,
        node: &crate::parser::node::Node,
    ) -> (Range, String) {
        let mut end = node.end;

        // Include trailing whitespace and newlines
        let mut trailing = String::new();
        if let Some(rest) = self.source.get(end as usize..) {
            // Capture all trailing whitespace
            let mut offset = 0usize;
            for &byte in rest.as_bytes() {
                if byte.is_ascii_whitespace() {
                    offset += 1;
                    if byte == b'\n' {
                        // Include the newline and stop
                        end += offset as u32;
                        trailing = "\n".to_string();
                        break;
                    }
                    if byte == b'\r' {
                        // Check for CRLF
                        if rest.as_bytes().get(offset) == Some(&b'\n') {
                            offset += 1;
                        }
                        end += offset as u32;
                        trailing = if offset == 2 { "\r\n" } else { "\r" }.to_string();
                        break;
                    }
                    continue;
                }
                // Found non-whitespace - include what we have and stop
                end += offset as u32;
                break;
            }

            // If we didn't find a newline, check if there's a semicolon
            if trailing.is_empty() {
                offset = 0;
                for &byte in rest.as_bytes() {
                    if byte == b';' {
                        end += (offset + 1) as u32;
                        break;
                    }
                    if !byte.is_ascii_whitespace() {
                        break;
                    }
                    offset += 1;
                }
            }
        }

        let start_pos = self.line_map.offset_to_position(node.pos, self.source);
        let end_pos = self.line_map.offset_to_position(end, self.source);

        (Range::new(start_pos, end_pos), trailing)
    }

    fn missing_property_quickfix(&self, diag: &LspDiagnostic) -> Option<CodeAction> {
        let code = diag.code?;
        if code
            != crate::checker::types::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE
        {
            return None;
        }

        let start_offset = self
            .line_map
            .position_to_offset(diag.range.start, self.source)?;
        let node_idx = find_node_at_offset(self.arena, start_offset);
        if node_idx.is_none() {
            return None;
        }

        let info = self.property_access_info(node_idx)?;
        let target_node = self.arena.get(info.target)?;

        let (edits, title) = if target_node.kind == SyntaxKind::ThisKeyword as u16 {
            let edits = self.class_property_edits(info.access_node, &info.property_text)?;
            let title = format!("Add property '{}' to class", info.property_name);
            (edits, title)
        } else if target_node.kind == SyntaxKind::Identifier as u16 {
            let symbol_id = self.binder.resolve_identifier(self.arena, info.target)?;
            let symbol = self.binder.symbols.get(symbol_id)?;
            let mut result = None;

            for &decl_idx in &symbol.declarations {
                let decl_node = self.arena.get(decl_idx)?;
                if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                    continue;
                }

                let decl = self.arena.get_variable_declaration(decl_node)?;
                if decl.initializer.is_none() {
                    continue;
                }

                let init_node = self.arena.get(decl.initializer)?;
                if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                    continue;
                }

                let literal = self.arena.get_literal_expr(init_node)?;
                let edits =
                    self.object_literal_property_edits(init_node, literal, &info.property_text)?;
                let title = format!("Add property '{}' to object literal", info.property_name);
                result = Some((edits, title));
                break;
            }

            result?
        } else {
            return None;
        };

        let mut changes = std::collections::HashMap::new();
        changes.insert(self.file_name.clone(), edits);

        Some(CodeAction {
            title,
            kind: CodeActionKind::QuickFix,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
        })
    }

    fn missing_import_quickfixes(
        &self,
        root: NodeIndex,
        diag: &LspDiagnostic,
        candidates: &[ImportCandidate],
    ) -> Vec<CodeAction> {
        let code = match diag.code {
            Some(code) => code,
            None => return Vec::new(),
        };
        if code != crate::checker::types::diagnostics::diagnostic_codes::CANNOT_FIND_NAME {
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
            if usage == ImportUsage::Type {
                resolved.is_type_only = true;
            }

            let Some(edits) = self.build_import_edit(root, &resolved) else {
                continue;
            };

            let mut changes = std::collections::HashMap::new();
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
            });
        }

        actions
    }

    /// Organize imports: sort contiguous import blocks by module specifier.
    fn organize_imports(&self, root: NodeIndex) -> Option<CodeAction> {
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

            if end_idx > start_idx + 1 {
                if let Some(edit) =
                    self.sort_imports_range(&statements[start_idx..end_idx], &source_file.comments)
                {
                    edits.push(edit);
                }
            }

            while i < statements.len() && !self.is_import_declaration(statements[i]) {
                i += 1;
            }
        }

        if edits.is_empty() {
            return None;
        }

        let mut changes = std::collections::HashMap::new();
        changes.insert(self.file_name.clone(), edits);

        Some(CodeAction {
            title: "Organize Imports".to_string(),
            kind: CodeActionKind::SourceOrganizeImports,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
        })
    }

    fn is_import_declaration(&self, node_idx: NodeIndex) -> bool {
        self.arena
            .get(node_idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::IMPORT_DECLARATION)
    }

    fn sort_imports_range(
        &self,
        import_nodes: &[NodeIndex],
        comments: &[crate::comments::CommentRange],
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
            let start = leading.first().map(|c| c.pos).unwrap_or(node.pos);

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

    fn import_removal_target(&self, node_idx: NodeIndex) -> Option<(NodeIndex, ImportRemoval)> {
        let mut current = node_idx;
        while !current.is_none() {
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
        while !current.is_none() {
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
        let local_ident = if !spec.name.is_none() {
            spec.name
        } else {
            spec.property_name
        };
        self.arena
            .get_identifier_text(local_ident)
            .map(|name| name.to_string())
    }

    fn build_import_removal_edit(
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

        let mut default_name = if !clause.name.is_none() {
            self.arena
                .get_identifier_text(clause.name)
                .map(|name| name.to_string())
        } else {
            None
        };

        let mut namespace_name = None;
        let mut named_specs = Vec::new();

        if !clause.named_bindings.is_none() {
            let bindings_node = self.arena.get(clause.named_bindings)?;
            if bindings_node.kind == SyntaxKind::Identifier as u16 {
                namespace_name = self
                    .arena
                    .get_identifier_text(clause.named_bindings)
                    .map(|name| name.to_string());
            } else if let Some(named) = self.arena.get_named_imports(bindings_node) {
                for &spec_idx in &named.elements.nodes {
                    let spec_node = self.arena.get(spec_idx)?;
                    let spec = self.arena.get_specifier(spec_node)?;
                    let import_ident = if !spec.property_name.is_none() {
                        spec.property_name
                    } else {
                        spec.name
                    };
                    let local_ident = if !spec.name.is_none() {
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
            let title = format!("Remove unused import '{}'", removed_name);
            return Some((edit, title));
        }

        let mut parts = Vec::new();
        if let Some(default_name) = default_name {
            parts.push(default_name);
        }
        if let Some(namespace_name) = namespace_name {
            parts.push(format!("* as {}", namespace_name));
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
        let title = format!("Remove unused import '{}'", removed_name);

        Some((edit, title))
    }

    fn import_decl_range(&self, node: &crate::parser::node::Node) -> (Range, String) {
        let mut end = node.end;
        if let Some(import_decl) = self.arena.get_import_decl(node) {
            if let Some(module_node) = self.arena.get(import_decl.module_specifier) {
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

    fn diagnostic_identifier_usage(&self, diag: &LspDiagnostic) -> Option<(String, ImportUsage)> {
        let start_offset = self
            .line_map
            .position_to_offset(diag.range.start, self.source)?;
        let node_idx = find_node_at_offset(self.arena, start_offset);
        if node_idx.is_none() {
            return None;
        }
        let node = self.arena.get(node_idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
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
        while !current.is_none() {
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

            if parent_node.kind == syntax_kind_ext::HERITAGE_CLAUSE {
                if let Some(usage) = self.import_usage_for_heritage_clause(parent_idx) {
                    return usage;
                }
            }

            if parent_node.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS {
                if let Some(usage) = self.import_usage_in_heritage(parent_idx) {
                    return usage;
                }
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
            if container_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || container_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                return Some(ImportUsage::Value);
            }
            return Some(ImportUsage::Type);
        }

        if heritage.token == SyntaxKind::ImplementsKeyword as u16 {
            return Some(ImportUsage::Type);
        }

        None
    }

    fn build_import_edit(
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

        let (insert_pos, needs_newline) = self.import_insertion_point(root)?;
        let mut new_text = String::new();
        if needs_newline {
            new_text.push('\n');
        }

        new_text.push_str("import ");
        if candidate.is_type_only {
            new_text.push_str("type ");
        }

        match &candidate.kind {
            ImportCandidateKind::Named { export_name } => {
                if export_name == &candidate.local_name {
                    new_text.push_str(&format!("{{ {} }}", export_name));
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
        new_text.push_str("\";\n");

        Some(vec![TextEdit {
            range: Range::new(insert_pos, insert_pos),
            new_text,
        }])
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
            if module_text != candidate.module_specifier {
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

            if !clause.name.is_none() {
                if let Some(name) = self.arena.get_identifier_text(clause.name) {
                    if name == candidate.local_name {
                        return MergeDefaultImport::AlreadyImported;
                    }
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

        let mut default_target = None;

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
            if module_text != candidate.module_specifier {
                continue;
            }

            let Some(clause_node) = self.arena.get(import_decl.import_clause) else {
                continue;
            };
            let Some(clause) = self.arena.get_import_clause(clause_node) else {
                continue;
            };
            if clause.is_type_only && !candidate.is_type_only {
                continue;
            }

            if !clause.named_bindings.is_none() {
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
                    if let Some(edits) =
                        self.build_named_import_insertion_edits(bindings_idx, named, &spec_text)
                    {
                        return MergeNamedImport::Edits(edits);
                    }
                    return MergeNamedImport::NoMatch;
                }
            } else {
                default_target = Some(stmt_idx);
            }
        }

        if let Some(import_idx) = default_target {
            if let Some(edit) = self.build_default_import_named_edit(import_idx, candidate) {
                return MergeNamedImport::Edits(vec![edit]);
            }
        }

        MergeNamedImport::NoMatch
    }

    fn named_imports_has_local_name(
        &self,
        named: &crate::parser::node::NamedImportsData,
        local_name: &str,
    ) -> bool {
        for &spec_idx in &named.elements.nodes {
            let Some(spec_node) = self.arena.get(spec_idx) else {
                continue;
            };
            let Some(spec) = self.arena.get_specifier(spec_node) else {
                continue;
            };
            let local_ident = if !spec.name.is_none() {
                spec.name
            } else {
                spec.property_name
            };
            if let Some(name) = self.arena.get_identifier_text(local_ident) {
                if name == local_name {
                    return true;
                }
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
        named: &crate::parser::node::NamedImportsData,
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
            let new_text = format!("{}{}{}", prefix, spec_text, trailing_space);
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
            format!("{}{}", close_indent, indent_unit)
        };

        if let Some(&last) = elements.last() {
            let last_node = self.arena.get(last)?;
            let last_spec = self.arena.get_specifier(last_node)?;
            let last_ident = if !last_spec.name.is_none() {
                last_spec.name
            } else {
                last_spec.property_name
            };
            let last_end = self
                .arena
                .get(last_ident)
                .map(|node| node.end)
                .unwrap_or(last_node.end);
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
        if clause.name.is_none() || !clause.named_bindings.is_none() {
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

    fn build_default_import_insertion_edit(
        &self,
        clause_node: &crate::parser::node::Node,
        clause: &crate::parser::node::ImportClauseData,
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

    fn import_insertion_point(&self, root: NodeIndex) -> Option<(Position, bool)> {
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

    fn property_access_info(&self, node_idx: NodeIndex) -> Option<PropertyAccessInfo> {
        let mut current = node_idx;
        while !current.is_none() {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let access = self.arena.get_access_expr(node)?;
                let name_node = self.arena.get(access.name_or_argument)?;
                if name_node.kind != SyntaxKind::Identifier as u16 {
                    return None;
                }
                let property_name = self
                    .arena
                    .get_identifier_text(access.name_or_argument)?
                    .to_string();
                return Some(PropertyAccessInfo {
                    access_node: current,
                    target: access.expression,
                    property_name: property_name.clone(),
                    property_text: property_name,
                });
            }

            if node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
                let access = self.arena.get_access_expr(node)?;
                let arg_node = self.arena.get(access.name_or_argument)?;
                let (property_name, property_text) = match arg_node.kind {
                    k if k == SyntaxKind::StringLiteral as u16
                        || k == SyntaxKind::NumericLiteral as u16 =>
                    {
                        let name = self
                            .arena
                            .get_literal_text(access.name_or_argument)?
                            .to_string();
                        let text = self
                            .source
                            .get(arg_node.pos as usize..arg_node.end as usize)?
                            .to_string();
                        (name, text)
                    }
                    _ => return None,
                };
                return Some(PropertyAccessInfo {
                    access_node: current,
                    target: access.expression,
                    property_name,
                    property_text,
                });
            }
            current = self.arena.get_extended(current)?.parent;
        }

        None
    }

    fn object_literal_property_edits(
        &self,
        object_node: &crate::parser::node::Node,
        literal: &crate::parser::node::LiteralExprData,
        property_text: &str,
    ) -> Option<Vec<TextEdit>> {
        let close_offset = self.find_closing_brace_offset(object_node)?;
        let open_pos = self
            .line_map
            .offset_to_position(object_node.pos, self.source);
        let close_pos = self.line_map.offset_to_position(close_offset, self.source);
        let is_single_line = open_pos.line == close_pos.line;

        let mut edits = Vec::new();
        let elements = &literal.elements.nodes;

        if is_single_line {
            let mut insert_offset = close_offset;
            while insert_offset > object_node.pos {
                let idx = (insert_offset - 1) as usize;
                let ch = *self.source.as_bytes().get(idx)?;
                if !ch.is_ascii_whitespace() {
                    break;
                }
                insert_offset -= 1;
            }
            let had_trailing_ws = insert_offset != close_offset;
            let trailing_space = if had_trailing_ws { "" } else { " " };
            let last_char = if insert_offset > object_node.pos {
                self.source
                    .as_bytes()
                    .get((insert_offset - 1) as usize)
                    .copied()
            } else {
                None
            };
            let had_trailing_comma = matches!(last_char, Some(b','));
            let prefix = if elements.is_empty() {
                " "
            } else if had_trailing_comma {
                " "
            } else {
                ", "
            };
            let mut new_text = format!("{}{}: undefined", prefix, property_text);
            if had_trailing_comma {
                new_text.push(',');
            }
            new_text.push_str(trailing_space);
            let insert_pos = self.line_map.offset_to_position(insert_offset, self.source);
            edits.push(TextEdit {
                range: Range::new(insert_pos, insert_pos),
                new_text,
            });
            return Some(edits);
        }

        let close_line_start = self.line_map.line_start(close_pos.line as usize)?;
        let close_indent = self.indent_at_offset(close_line_start);
        let prop_indent = if let Some(&first) = elements.first() {
            let first_node = self.arena.get(first)?;
            self.indent_at_offset(first_node.pos)
        } else {
            let indent_unit = self.indent_unit_from(&close_indent);
            format!("{}{}", close_indent, indent_unit)
        };

        if let Some(&last) = elements.last() {
            let last_node = self.arena.get(last)?;
            let last_end = if last_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
                self.arena
                    .get_property_assignment(last_node)
                    .and_then(|prop| {
                        let tail = if prop.initializer.is_none() {
                            prop.name
                        } else {
                            prop.initializer
                        };
                        self.arena.get(tail).map(|node| node.end)
                    })
                    .unwrap_or(last_node.end)
            } else {
                last_node.end
            };
            let between = self.source.get(last_end as usize..close_offset as usize)?;
            let trimmed = between.trim_start();
            if trimmed.contains("//") || trimmed.contains("/*") {
                return None;
            }
            let had_trailing_comma = trimmed.starts_with(',');
            if !had_trailing_comma {
                let last_pos = self.line_map.offset_to_position(last_end, self.source);
                edits.push(TextEdit {
                    range: Range::new(last_pos, last_pos),
                    new_text: ",".to_string(),
                });
            }

            let mut line = String::new();
            line.push_str(&prop_indent);
            line.push_str(property_text);
            line.push_str(": undefined");
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
        line.push_str(&prop_indent);
        line.push_str(property_text);
        line.push_str(": undefined\n");
        let insert_pos = self
            .line_map
            .offset_to_position(close_line_start, self.source);
        edits.push(TextEdit {
            range: Range::new(insert_pos, insert_pos),
            new_text: line,
        });

        Some(edits)
    }

    fn class_property_edits(
        &self,
        node_idx: NodeIndex,
        property_text: &str,
    ) -> Option<Vec<TextEdit>> {
        let class_idx = self.find_enclosing_class(node_idx)?;
        let class_node = self.arena.get(class_idx)?;
        let class_data = self.arena.get_class(class_node)?;

        let close_offset = self.find_closing_brace_offset(class_node)?;
        let open_pos = self
            .line_map
            .offset_to_position(class_node.pos, self.source);
        let close_pos = self.line_map.offset_to_position(close_offset, self.source);
        let is_single_line = open_pos.line == close_pos.line;

        let mut edits = Vec::new();
        if is_single_line {
            let mut insert_offset = close_offset;
            while insert_offset > class_node.pos {
                let idx = (insert_offset - 1) as usize;
                let ch = *self.source.as_bytes().get(idx)?;
                if !ch.is_ascii_whitespace() {
                    break;
                }
                insert_offset -= 1;
            }
            let had_trailing_ws = insert_offset != close_offset;
            let trailing_space = if had_trailing_ws { "" } else { " " };
            let new_text = format!(" {}: any;{}", property_text, trailing_space);
            let insert_pos = self.line_map.offset_to_position(insert_offset, self.source);
            edits.push(TextEdit {
                range: Range::new(insert_pos, insert_pos),
                new_text,
            });
            return Some(edits);
        }

        let close_line_start = self.line_map.line_start(close_pos.line as usize)?;
        let close_indent = self.indent_at_offset(close_line_start);
        let prop_indent = if let Some(&first) = class_data.members.nodes.first() {
            let first_node = self.arena.get(first)?;
            self.indent_at_offset(first_node.pos)
        } else {
            let indent_unit = self.indent_unit_from(&close_indent);
            format!("{}{}", close_indent, indent_unit)
        };

        let mut line = String::new();
        line.push_str(&prop_indent);
        line.push_str(property_text);
        line.push_str(": any;\n");

        let insert_pos = self
            .line_map
            .offset_to_position(close_line_start, self.source);
        edits.push(TextEdit {
            range: Range::new(insert_pos, insert_pos),
            new_text: line,
        });

        Some(edits)
    }

    fn find_enclosing_class(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = node_idx;
        while !current.is_none() {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::CLASS_DECLARATION
                || node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                return Some(current);
            }
            current = self.arena.get_extended(current)?.parent;
        }
        None
    }

    fn find_closing_brace_offset(&self, node: &crate::parser::node::Node) -> Option<u32> {
        let slice = self.source.get(node.pos as usize..node.end as usize)?;
        let rel = slice.rfind('}')?;
        Some(node.pos + rel as u32)
    }

    fn indent_at_offset(&self, offset: u32) -> String {
        let pos = self.line_map.offset_to_position(offset, self.source);
        self.get_indentation_at_position(&Position::new(pos.line, 0))
    }

    fn indent_unit_from(&self, base_indent: &str) -> &str {
        if base_indent.contains('\t') {
            "\t"
        } else {
            "  "
        }
    }

    /// Extract the selected expression to a new variable.
    ///
    /// Example: Selecting `foo.bar.baz` produces:
    /// ```typescript
    /// const extracted = foo.bar.baz;
    /// // ... use extracted here
    /// ```
    fn extract_variable(&self, root: NodeIndex, range: Range) -> Option<CodeAction> {
        // 1. Convert range to offsets
        let start_offset = self.line_map.position_to_offset(range.start, self.source)?;
        let end_offset = self.line_map.position_to_offset(range.end, self.source)?;

        // 2. Find the expression node that matches this range
        let expr_idx = self.find_expression_at_range(root, start_offset, end_offset)?;

        // 3. Verify it's an expression (not a statement or declaration)
        let expr_node = self.arena.get(expr_idx)?;
        if !self.is_extractable_expression(expr_node.kind) {
            return None;
        }

        // 4. Find the enclosing statement to determine where to insert the variable
        let stmt_idx = self.find_enclosing_statement(root, expr_idx)?;
        let stmt_node = self.arena.get(stmt_idx)?;
        if !self.statement_allows_lexical_insertion(stmt_idx) {
            return None;
        }
        if !self.expression_and_statement_share_scope(expr_idx, stmt_idx) {
            return None;
        }
        if self.extraction_has_tdz_violation(expr_idx, stmt_idx) {
            return None;
        }

        // 5. Generate a unique variable name scoped to the insertion point.
        let var_name = self.unique_extracted_name(stmt_idx);

        // 6. Extract the selected text (snap to node boundaries)
        let (node_start, node_end) = self.expression_text_span(expr_idx, expr_node);
        let selected_text = self.source.get(node_start as usize..node_end as usize)?;
        let initializer_text = self.format_extracted_initializer(expr_node, selected_text);
        let replacement_range = Range::new(
            self.line_map.offset_to_position(node_start, self.source),
            self.line_map.offset_to_position(node_end, self.source),
        );

        // 7. Create text edits:
        //    a) Insert variable declaration before the statement
        //    b) Replace the selected expression with the variable name

        // Get the position to insert the variable declaration
        let stmt_pos = self.line_map.offset_to_position(stmt_node.pos, self.source);
        let insert_pos = Position::new(stmt_pos.line, 0);

        // Calculate indentation by looking at the statement's line
        let indent = self.get_indentation_at_position(&stmt_pos);

        let declaration = format!("{}const {} = {};\n", indent, var_name, initializer_text);

        let mut edits = Vec::new();

        // Insert the declaration
        edits.push(TextEdit {
            range: Range {
                start: insert_pos,
                end: insert_pos,
            },
            new_text: declaration,
        });

        // Replace the expression with the variable name
        let mut replacement_text = if self.needs_jsx_expression_wrapper(expr_idx) {
            format!("{{{}}}", var_name)
        } else {
            var_name.clone()
        };
        if self.should_preserve_parenthesized_replacement(expr_node) {
            replacement_text = format!("({})", replacement_text);
        }
        edits.push(TextEdit {
            range: replacement_range,
            new_text: replacement_text,
        });

        // Create the workspace edit
        let mut changes = std::collections::HashMap::new();
        changes.insert(self.file_name.clone(), edits);

        Some(CodeAction {
            title: format!("Extract to constant '{}'", var_name),
            kind: CodeActionKind::RefactorExtract,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: true,
        })
    }

    fn unique_extracted_name(&self, stmt_idx: NodeIndex) -> String {
        let mut names = FxHashSet::default();
        if let Some(scope_id) = self.find_enclosing_scope_id(stmt_idx) {
            self.collect_scope_names(scope_id, &mut names);
        }

        let base = "extracted";
        if !names.contains(base) {
            return base.to_string();
        }

        let mut suffix = 2;
        loop {
            let candidate = format!("{}{}", base, suffix);
            if !names.contains(&candidate) {
                return candidate;
            }
            suffix += 1;
        }
    }

    fn format_extracted_initializer(&self, expr_node: &Node, selected_text: &str) -> String {
        if self.needs_parentheses_for_extraction(expr_node) {
            if expr_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                return selected_text.to_string();
            }
            return format!("({})", selected_text);
        }
        selected_text.to_string()
    }

    fn expression_text_span(&self, expr_idx: NodeIndex, expr_node: &Node) -> (u32, u32) {
        let mut start = expr_node.pos;
        let mut end = expr_node.end;

        if expr_node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            if let Some(binary) = self.arena.get_binary_expr(expr_node) {
                start = self
                    .arena
                    .get(binary.left)
                    .map(|node| node.pos)
                    .unwrap_or(start);
                end = self
                    .arena
                    .get(binary.right)
                    .map(|node| node.end)
                    .unwrap_or(end);
            }
        }

        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            if let Some(access) = self.arena.get_access_expr(expr_node) {
                if let Some(name_node) = self.arena.get(access.name_or_argument) {
                    end = name_node.end;
                }
            }
        }

        if let Some(ext) = self.arena.get_extended(expr_idx) {
            if let Some(parent_node) = self.arena.get(ext.parent) {
                if parent_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                    let parent_start = parent_node.pos as usize;
                    let parent_end = parent_node.end as usize;
                    if let Some(slice) = self.source.get(parent_start..parent_end) {
                        if let Some(open_rel) = slice.find('(') {
                            let open_pos = parent_node.pos + open_rel as u32;
                            if start <= open_pos {
                                start = open_pos.saturating_add(1);
                            }
                        }
                        if let Some(close_rel) = slice.rfind(')') {
                            let close_pos = parent_node.pos + close_rel as u32;
                            if end > close_pos {
                                end = close_pos;
                            }
                        }
                    }
                }
            }
        }

        (start, end)
    }

    fn needs_parentheses_for_extraction(&self, expr_node: &Node) -> bool {
        if expr_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            if let Some(paren) = self.arena.get_parenthesized(expr_node) {
                if let Some(inner) = self.arena.get(paren.expression) {
                    return self.needs_parentheses_for_extraction(inner);
                }
            }
            return false;
        }

        if expr_node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            if let Some(binary) = self.arena.get_binary_expr(expr_node) {
                return binary.operator_token == SyntaxKind::CommaToken as u16;
            }
        }

        false
    }

    fn should_preserve_parenthesized_replacement(&self, expr_node: &Node) -> bool {
        if expr_node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            return false;
        }

        let Some(paren) = self.arena.get_parenthesized(expr_node) else {
            return true;
        };
        let Some(inner) = self.arena.get(paren.expression) else {
            return true;
        };

        !self.is_comma_expression(inner)
    }

    fn is_comma_expression(&self, expr_node: &Node) -> bool {
        if expr_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            if let Some(paren) = self.arena.get_parenthesized(expr_node) {
                if let Some(inner) = self.arena.get(paren.expression) {
                    return self.is_comma_expression(inner);
                }
            }
            return false;
        }

        if expr_node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            if let Some(binary) = self.arena.get_binary_expr(expr_node) {
                return binary.operator_token == SyntaxKind::CommaToken as u16;
            }
        }

        false
    }

    fn needs_jsx_expression_wrapper(&self, expr_idx: NodeIndex) -> bool {
        let node = match self.arena.get(expr_idx) {
            Some(node) => node,
            None => return false,
        };

        if node.kind != syntax_kind_ext::JSX_ELEMENT
            && node.kind != syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
            && node.kind != syntax_kind_ext::JSX_FRAGMENT
        {
            return false;
        }

        let parent = match self.arena.get_extended(expr_idx) {
            Some(ext) => ext.parent,
            None => return false,
        };
        if parent.is_none() {
            return false;
        }

        let parent_node = match self.arena.get(parent) {
            Some(node) => node,
            None => return false,
        };

        parent_node.kind == syntax_kind_ext::JSX_ELEMENT
            || parent_node.kind == syntax_kind_ext::JSX_FRAGMENT
    }

    fn expression_and_statement_share_scope(
        &self,
        expr_idx: NodeIndex,
        stmt_idx: NodeIndex,
    ) -> bool {
        let expr_scope = match self.find_enclosing_scope_id(expr_idx) {
            Some(scope_id) => scope_id,
            None => return true,
        };
        let stmt_scope = match self.find_enclosing_scope_id(stmt_idx) {
            Some(scope_id) => scope_id,
            None => return true,
        };
        expr_scope == stmt_scope
    }

    fn extraction_has_tdz_violation(&self, expr_idx: NodeIndex, stmt_idx: NodeIndex) -> bool {
        let stmt_node = match self.arena.get(stmt_idx) {
            Some(node) => node,
            None => return false,
        };
        let insertion_pos = stmt_node.pos;

        let mut identifiers = Vec::new();
        self.collect_identifier_uses_in_expression(expr_idx, &mut identifiers);
        if identifiers.is_empty() {
            return false;
        }

        let mut seen_symbols = FxHashSet::default();
        for ident_idx in identifiers {
            let Some(sym_id) = self.binder.resolve_identifier(self.arena, ident_idx) else {
                continue;
            };
            if !seen_symbols.insert(sym_id) {
                continue;
            }
            if self.symbol_has_tdz_after(sym_id, insertion_pos) {
                return true;
            }
        }

        false
    }

    fn symbol_has_tdz_after(&self, sym_id: SymbolId, insertion_pos: u32) -> bool {
        let Some(symbol) = self.binder.symbols.get(sym_id) else {
            return false;
        };
        if !self.symbol_is_lexical(symbol.flags) {
            return false;
        }

        let mut earliest_decl: Option<u32> = None;
        for decl_idx in &symbol.declarations {
            let Some(decl_node) = self.arena.get(*decl_idx) else {
                continue;
            };
            earliest_decl = Some(match earliest_decl {
                Some(pos) => pos.min(decl_node.pos),
                None => decl_node.pos,
            });
        }

        matches!(earliest_decl, Some(pos) if pos > insertion_pos)
    }

    fn symbol_is_lexical(&self, flags: u32) -> bool {
        (flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0 || (flags & symbol_flags::CLASS) != 0
    }

    fn collect_identifier_uses_in_expression(&self, expr_idx: NodeIndex, out: &mut Vec<NodeIndex>) {
        if expr_idx.is_none() {
            return;
        }

        let Some(node) = self.arena.get(expr_idx) else {
            return;
        };

        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                out.push(expr_idx);
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node) {
                    self.collect_identifier_uses_in_expression(access.expression, out);
                }
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node) {
                    self.collect_identifier_uses_in_expression(access.expression, out);
                    self.collect_identifier_uses_in_expression(access.name_or_argument, out);
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION || k == syntax_kind_ext::NEW_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(node) {
                    self.collect_identifier_uses_in_expression(call.expression, out);
                    if let Some(args) = &call.arguments {
                        for &arg in &args.nodes {
                            self.collect_identifier_uses_in_expression(arg, out);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(binary) = self.arena.get_binary_expr(node) {
                    self.collect_identifier_uses_in_expression(binary.left, out);
                    self.collect_identifier_uses_in_expression(binary.right, out);
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                || k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION =>
            {
                if let Some(unary) = self.arena.get_unary_expr(node) {
                    self.collect_identifier_uses_in_expression(unary.operand, out);
                }
            }
            k if k == syntax_kind_ext::AWAIT_EXPRESSION
                || k == syntax_kind_ext::YIELD_EXPRESSION
                || k == syntax_kind_ext::NON_NULL_EXPRESSION
                || k == syntax_kind_ext::SPREAD_ELEMENT
                || k == syntax_kind_ext::SPREAD_ASSIGNMENT =>
            {
                if node.has_data() {
                    if let Some(unary) = self.arena.unary_exprs_ex.get(node.data_index as usize) {
                        self.collect_identifier_uses_in_expression(unary.expression, out);
                    }
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = self.arena.get_conditional_expr(node) {
                    self.collect_identifier_uses_in_expression(cond.condition, out);
                    self.collect_identifier_uses_in_expression(cond.when_true, out);
                    self.collect_identifier_uses_in_expression(cond.when_false, out);
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.arena.get_parenthesized(node) {
                    self.collect_identifier_uses_in_expression(paren.expression, out);
                }
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                if let Some(literal) = self.arena.get_literal_expr(node) {
                    for &elem in &literal.elements.nodes {
                        self.collect_identifier_uses_in_expression(elem, out);
                    }
                }
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                if let Some(literal) = self.arena.get_literal_expr(node) {
                    for &elem in &literal.elements.nodes {
                        self.collect_identifier_uses_in_object_literal_element(elem, out);
                    }
                }
            }
            k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => {
                if node.has_data() {
                    if let Some(tagged) = self.arena.tagged_templates.get(node.data_index as usize)
                    {
                        self.collect_identifier_uses_in_expression(tagged.tag, out);
                        self.collect_identifier_uses_in_expression(tagged.template, out);
                    }
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                if let Some(template) = self.arena.get_template_expr(node) {
                    for &span_idx in &template.template_spans.nodes {
                        let Some(span_node) = self.arena.get(span_idx) else {
                            continue;
                        };
                        if let Some(span) = self.arena.get_template_span(span_node) {
                            self.collect_identifier_uses_in_expression(span.expression, out);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                if node.has_data() {
                    if let Some(assertion) =
                        self.arena.type_assertions.get(node.data_index as usize)
                    {
                        self.collect_identifier_uses_in_expression(assertion.expression, out);
                    }
                }
            }
            k if k == syntax_kind_ext::JSX_ELEMENT => {
                if let Some(jsx) = self.arena.get_jsx_element(node) {
                    self.collect_identifier_uses_in_jsx_opening(jsx.opening_element, out);
                    for &child in &jsx.children.nodes {
                        self.collect_identifier_uses_in_jsx_child(child, out);
                    }
                }
            }
            k if k == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT => {
                self.collect_identifier_uses_in_jsx_opening(expr_idx, out);
            }
            k if k == syntax_kind_ext::JSX_FRAGMENT => {
                if let Some(fragment) = self.arena.get_jsx_fragment(node) {
                    for &child in &fragment.children.nodes {
                        self.collect_identifier_uses_in_jsx_child(child, out);
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::CLASS_EXPRESSION =>
            {
                // Skip nested scopes to avoid capturing non-evaluated identifiers.
            }
            _ => {}
        }
    }

    fn collect_identifier_uses_in_object_literal_element(
        &self,
        element_idx: NodeIndex,
        out: &mut Vec<NodeIndex>,
    ) {
        let Some(element_node) = self.arena.get(element_idx) else {
            return;
        };

        match element_node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                if let Some(prop) = self.arena.get_property_assignment(element_node) {
                    self.collect_identifier_uses_in_computed_property_name(prop.name, out);
                    self.collect_identifier_uses_in_expression(prop.initializer, out);
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.arena.get_method_decl(element_node) {
                    self.collect_identifier_uses_in_computed_property_name(method.name, out);
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(element_node) {
                    self.collect_identifier_uses_in_computed_property_name(accessor.name, out);
                }
            }
            _ => {
                self.collect_identifier_uses_in_expression(element_idx, out);
            }
        }
    }

    fn collect_identifier_uses_in_computed_property_name(
        &self,
        name_idx: NodeIndex,
        out: &mut Vec<NodeIndex>,
    ) {
        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };

        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            if let Some(computed) = self.arena.get_computed_property(name_node) {
                self.collect_identifier_uses_in_expression(computed.expression, out);
            }
        }
    }

    fn collect_identifier_uses_in_jsx_opening(
        &self,
        opening_idx: NodeIndex,
        out: &mut Vec<NodeIndex>,
    ) {
        let Some(opening_node) = self.arena.get(opening_idx) else {
            return;
        };
        let Some(opening) = self.arena.get_jsx_opening(opening_node) else {
            return;
        };

        self.collect_identifier_uses_in_jsx_tag_name(opening.tag_name, out);
        self.collect_identifier_uses_in_jsx_attributes(opening.attributes, out);
    }

    fn collect_identifier_uses_in_jsx_tag_name(
        &self,
        tag_idx: NodeIndex,
        out: &mut Vec<NodeIndex>,
    ) {
        let Some(tag_node) = self.arena.get(tag_idx) else {
            return;
        };

        match tag_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(name) = self.arena.get_identifier_text(tag_idx) {
                    if Self::jsx_tag_is_component(name) {
                        out.push(tag_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::JSX_NAMESPACED_NAME => {
                // Namespaced JSX names are intrinsic; skip to avoid false TDZ positives.
            }
            _ => {
                self.collect_identifier_uses_in_expression(tag_idx, out);
            }
        }
    }

    fn collect_identifier_uses_in_jsx_attributes(
        &self,
        attrs_idx: NodeIndex,
        out: &mut Vec<NodeIndex>,
    ) {
        let Some(attrs_node) = self.arena.get(attrs_idx) else {
            return;
        };
        let Some(attrs) = self.arena.get_jsx_attributes(attrs_node) else {
            return;
        };

        for &prop in &attrs.properties.nodes {
            let Some(prop_node) = self.arena.get(prop) else {
                continue;
            };
            match prop_node.kind {
                k if k == syntax_kind_ext::JSX_ATTRIBUTE => {
                    if let Some(attr) = self.arena.get_jsx_attribute(prop_node) {
                        if attr.initializer.is_none() {
                            continue;
                        }
                        self.collect_identifier_uses_in_jsx_attribute_initializer(
                            attr.initializer,
                            out,
                        );
                    }
                }
                k if k == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE => {
                    if let Some(spread) = self.arena.get_jsx_spread_attribute(prop_node) {
                        self.collect_identifier_uses_in_expression(spread.expression, out);
                    }
                }
                _ => {}
            }
        }
    }

    fn collect_identifier_uses_in_jsx_attribute_initializer(
        &self,
        init_idx: NodeIndex,
        out: &mut Vec<NodeIndex>,
    ) {
        let Some(init_node) = self.arena.get(init_idx) else {
            return;
        };

        if init_node.kind == syntax_kind_ext::JSX_EXPRESSION {
            if let Some(expr) = self.arena.get_jsx_expression(init_node) {
                self.collect_identifier_uses_in_expression(expr.expression, out);
            }
            return;
        }

        self.collect_identifier_uses_in_expression(init_idx, out);
    }

    fn collect_identifier_uses_in_jsx_child(&self, child_idx: NodeIndex, out: &mut Vec<NodeIndex>) {
        let Some(child_node) = self.arena.get(child_idx) else {
            return;
        };

        match child_node.kind {
            k if k == syntax_kind_ext::JSX_EXPRESSION => {
                if let Some(expr) = self.arena.get_jsx_expression(child_node) {
                    self.collect_identifier_uses_in_expression(expr.expression, out);
                }
            }
            k if k == syntax_kind_ext::JSX_ELEMENT
                || k == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
                || k == syntax_kind_ext::JSX_FRAGMENT =>
            {
                self.collect_identifier_uses_in_expression(child_idx, out);
            }
            _ => {}
        }
    }

    fn jsx_tag_is_component(name: &str) -> bool {
        name.chars()
            .next()
            .map(|ch| ch.is_ascii_uppercase())
            .unwrap_or(false)
    }

    fn find_enclosing_scope_id(&self, node_idx: NodeIndex) -> Option<ScopeId> {
        let mut current = node_idx;
        while !current.is_none() {
            if let Some(&scope_id) = self.binder.node_scope_ids.get(&current.0) {
                return Some(scope_id);
            }
            let ext = match self.arena.get_extended(current) {
                Some(ext) => ext,
                None => break,
            };
            current = ext.parent;
        }

        if !self.binder.scopes.is_empty() {
            Some(ScopeId(0))
        } else {
            None
        }
    }

    fn collect_scope_names(&self, mut scope_id: ScopeId, names: &mut FxHashSet<String>) {
        while !scope_id.is_none() {
            let scope = match self.binder.scopes.get(scope_id.0 as usize) {
                Some(scope) => scope,
                None => break,
            };
            names.extend(scope.table.iter().map(|(name, _)| name.clone()));
            scope_id = scope.parent;
        }
    }

    /// Find an expression node that matches the given range.
    /// Finds the smallest expression node that contains the selection.
    fn find_expression_at_range(
        &self,
        _root: NodeIndex,
        start: u32,
        end: u32,
    ) -> Option<NodeIndex> {
        let mut current = find_node_at_offset(self.arena, start);
        while !current.is_none() {
            let node = self.arena.get(current)?;
            if node.pos <= start && node.end >= end && self.is_expression(node.kind) {
                return Some(current);
            }
            let ext = self.arena.get_extended(current)?;
            current = ext.parent;
        }

        None
    }

    /// Find the enclosing statement for a given node.
    fn find_enclosing_statement(&self, _root: NodeIndex, node_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = node_idx;
        while !current.is_none() {
            let node = self.arena.get(current)?;
            if self.is_statement(node.kind) {
                return Some(current);
            }
            let ext = self.arena.get_extended(current)?;
            current = ext.parent;
        }
        None
    }

    /// Check if a syntax kind is an expression.
    fn is_expression(&self, kind: u16) -> bool {
        // Check both token kinds (from scanner) and expression kinds (from parser)
        kind == SyntaxKind::Identifier as u16
            || kind == SyntaxKind::StringLiteral as u16
            || kind == SyntaxKind::NumericLiteral as u16
            || kind == SyntaxKind::TrueKeyword as u16
            || kind == SyntaxKind::FalseKeyword as u16
            || kind == SyntaxKind::NullKeyword as u16
            || kind == SyntaxKind::ThisKeyword as u16
            || kind == SyntaxKind::SuperKeyword as u16
            || kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || kind == syntax_kind_ext::CALL_EXPRESSION
            || kind == syntax_kind_ext::BINARY_EXPRESSION
            || kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            || kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            || kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || kind == syntax_kind_ext::ARROW_FUNCTION
            || kind == syntax_kind_ext::CLASS_EXPRESSION
            || kind == syntax_kind_ext::NEW_EXPRESSION
            || kind == syntax_kind_ext::TEMPLATE_EXPRESSION
            || kind == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION
            || kind == syntax_kind_ext::AWAIT_EXPRESSION
            || kind == syntax_kind_ext::YIELD_EXPRESSION
            || kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
            || kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            || kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            || kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION
            || kind == syntax_kind_ext::JSX_ELEMENT
            || kind == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
            || kind == syntax_kind_ext::JSX_FRAGMENT
    }

    /// Check if an expression is extractable (not all expressions should be extracted).
    fn is_extractable_expression(&self, kind: u16) -> bool {
        // Don't extract simple literals or identifiers - not useful
        !(kind == SyntaxKind::Identifier as u16
            || kind == SyntaxKind::StringLiteral as u16
            || kind == SyntaxKind::NumericLiteral as u16
            || kind == SyntaxKind::BigIntLiteral as u16
            || kind == SyntaxKind::TrueKeyword as u16
            || kind == SyntaxKind::FalseKeyword as u16
            || kind == SyntaxKind::NullKeyword as u16)
    }

    /// Check if a syntax kind is a statement.
    fn is_statement(&self, kind: u16) -> bool {
        matches!(
            kind,
            syntax_kind_ext::VARIABLE_STATEMENT
                | syntax_kind_ext::EXPRESSION_STATEMENT
                | syntax_kind_ext::IF_STATEMENT
                | syntax_kind_ext::FOR_STATEMENT
                | syntax_kind_ext::FOR_IN_STATEMENT
                | syntax_kind_ext::FOR_OF_STATEMENT
                | syntax_kind_ext::WHILE_STATEMENT
                | syntax_kind_ext::DO_STATEMENT
                | syntax_kind_ext::RETURN_STATEMENT
                | syntax_kind_ext::BREAK_STATEMENT
                | syntax_kind_ext::CONTINUE_STATEMENT
                | syntax_kind_ext::THROW_STATEMENT
                | syntax_kind_ext::TRY_STATEMENT
                | syntax_kind_ext::SWITCH_STATEMENT
                | syntax_kind_ext::BLOCK
        )
    }

    fn statement_allows_lexical_insertion(&self, stmt_idx: NodeIndex) -> bool {
        let parent = match self.arena.get_extended(stmt_idx) {
            Some(ext) => ext.parent,
            None => return false,
        };
        if parent.is_none() {
            return true;
        }

        let parent_node = match self.arena.get(parent) {
            Some(node) => node,
            None => return false,
        };

        matches!(
            parent_node.kind,
            k if k == syntax_kind_ext::SOURCE_FILE
                || k == syntax_kind_ext::BLOCK
                || k == syntax_kind_ext::MODULE_BLOCK
                || k == syntax_kind_ext::CASE_CLAUSE
                || k == syntax_kind_ext::DEFAULT_CLAUSE
        )
    }

    /// Get the indentation (leading whitespace) at a given position.
    fn get_indentation_at_position(&self, pos: &Position) -> String {
        let line_start = self.line_map.line_start(pos.line as usize).unwrap_or(0);
        let slice = self.source.get(line_start as usize..).unwrap_or("");
        slice
            .chars()
            .take_while(|ch| *ch == ' ' || *ch == '\t')
            .collect()
    }
}

#[derive(Clone, Debug)]
struct NamedImportSpec {
    specifier: NodeIndex,
    import_name: String,
    local_name: String,
    is_type_only: bool,
}

#[derive(Clone, Debug)]
struct PropertyAccessInfo {
    access_node: NodeIndex,
    target: NodeIndex,
    property_name: String,
    property_text: String,
}

#[derive(Clone, Debug)]
enum ImportRemoval {
    Default { name: String },
    Namespace { name: String },
    Named { specifier: NodeIndex, name: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ImportUsage {
    Type,
    Value,
}

#[derive(Clone, Debug)]
enum MergeNamedImport {
    Edits(Vec<TextEdit>),
    AlreadyImported,
    NoMatch,
}

#[derive(Clone, Debug)]
enum MergeDefaultImport {
    Edits(Vec<TextEdit>),
    AlreadyImported,
    NoMatch,
}

impl ImportRemoval {
    fn name(&self) -> &str {
        match self {
            ImportRemoval::Default { name }
            | ImportRemoval::Namespace { name }
            | ImportRemoval::Named { name, .. } => name,
        }
    }
}
