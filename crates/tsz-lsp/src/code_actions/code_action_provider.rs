//! Code Actions for the LSP.
//!
//! Provides quick fixes and refactorings to improve code quality and fix errors.
//!
//! Architecture:
//! - The Checker identifies problems (Diagnostics)
//! - The `CodeActionProvider` identifies solutions (`TextEdits`)
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

use crate::diagnostics::LspDiagnostic;
use crate::rename::{TextEdit, WorkspaceEdit};
use crate::utils::find_node_at_offset;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use tsz_binder::BinderState;
use tsz_common::position::{LineMap, Position, Range};
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::{NodeAccess, NodeArena};
use tsz_parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

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
    /// Rewrite-style refactoring (convert syntax form).
    #[serde(rename = "refactor.rewrite")]
    RefactorRewrite,
    /// Generic source action.
    #[serde(rename = "source")]
    Source,
    /// Organize imports.
    #[serde(rename = "source.organizeImports")]
    SourceOrganizeImports,
    /// Add missing imports on save.
    #[serde(rename = "source.addMissingImports")]
    SourceAddMissingImports,
    /// Remove unused imports on save.
    #[serde(rename = "source.removeUnusedImports")]
    SourceRemoveUnusedImports,
    /// Sort imports on save.
    #[serde(rename = "source.sortImports")]
    SourceSortImports,
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
    pub const fn named(module_specifier: String, export_name: String, local_name: String) -> Self {
        Self {
            module_specifier,
            local_name,
            kind: ImportCandidateKind::Named { export_name },
            is_type_only: false,
        }
    }

    pub const fn default(module_specifier: String, local_name: String) -> Self {
        Self {
            module_specifier,
            local_name,
            kind: ImportCandidateKind::Default,
            is_type_only: false,
        }
    }

    pub const fn namespace(module_specifier: String, local_name: String) -> Self {
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
    /// Metadata for the action (e.g. fixId for `TSServer`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
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
    pub(super) arena: &'a NodeArena,
    pub(super) binder: &'a BinderState,
    pub(super) line_map: &'a LineMap,
    pub(super) file_name: String,
    pub(super) source: &'a str,
    pub(super) organize_imports_ignore_case: bool,
    pub(super) new_line_override: Option<String>,
}

impl<'a> CodeActionProvider<'a> {
    /// Create a new code action provider.
    pub const fn new(
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
            organize_imports_ignore_case: true,
            new_line_override: None,
        }
    }

    /// Build the provider from a borrowed
    /// [`crate::project::LspProviderContext`]. Equivalent to
    /// [`Self::new`] with default builder options.
    pub fn from_context(ctx: crate::project::LspProviderContext<'a>) -> Self {
        Self {
            arena: ctx.arena,
            binder: ctx.binder,
            line_map: ctx.line_map,
            file_name: ctx.file_name.to_string(),
            source: ctx.source_text,
            organize_imports_ignore_case: true,
            new_line_override: None,
        }
    }

    pub const fn with_organize_imports_ignore_case(mut self, ignore_case: bool) -> Self {
        self.organize_imports_ignore_case = ignore_case;
        self
    }

    /// Override the newline character used by inserted import edits. When
    /// unset, the provider picks between CRLF/LF by inspecting the source
    /// text (defaulting to CRLF when the source has no newlines).
    pub fn with_new_line_override(mut self, new_line: Option<String>) -> Self {
        self.new_line_override = new_line;
        self
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
                if let Some(action) = self.add_missing_const_quickfix(diag) {
                    actions.push(action);
                }
                actions.extend(self.missing_import_quickfixes(
                    root,
                    diag,
                    &context.import_candidates,
                ));

                // New diagnostic-driven quick fixes
                if let Some(action) = self.add_missing_await_quickfix(diag) {
                    actions.push(action);
                }
                if let Some(action) = self.convert_require_to_import_quickfix(diag) {
                    actions.push(action);
                }
                if let Some(action) = self.add_override_modifier_quickfix(diag) {
                    actions.push(action);
                }
                if let Some(action) = self.fix_spelling_quickfix(diag) {
                    actions.push(action);
                }
                if let Some(action) = self.prefix_unused_with_underscore_quickfix(diag) {
                    actions.push(action);
                }
            }
        }

        // Source Actions (file-level)
        let request_source = context.only.as_ref().is_none_or(|kinds| {
            kinds.contains(&CodeActionKind::Source)
                || kinds.contains(&CodeActionKind::SourceOrganizeImports)
                || kinds.contains(&CodeActionKind::SourceAddMissingImports)
                || kinds.contains(&CodeActionKind::SourceRemoveUnusedImports)
                || kinds.contains(&CodeActionKind::SourceSortImports)
        });
        if request_source {
            if let Some(action) = self.organize_imports(root) {
                actions.push(action);
            }
            // Additional source actions (remove unused, sort-only, etc.)
            actions.extend(self.source_actions(root));
        }

        // Refactorings
        let request_refactor = context.only.as_ref().is_none_or(|kinds| {
            kinds.contains(&CodeActionKind::Refactor)
                || kinds.contains(&CodeActionKind::RefactorExtract)
                || kinds.contains(&CodeActionKind::RefactorInline)
                || kinds.contains(&CodeActionKind::RefactorRewrite)
        });

        if request_refactor {
            // Extract refactorings require a non-empty selection
            if range.start != range.end {
                if let Some(action) = self.extract_variable(root, range) {
                    actions.push(action);
                }
                if let Some(action) = self.extract_function(root, range) {
                    actions.push(action);
                }
                if let Some(action) = self.extract_type_alias(root, range) {
                    actions.push(action);
                }
                actions.extend(self.surround_with_actions(root, range));
            }

            // Point refactorings (cursor position)
            if let Some(action) = self.convert_to_arrow_function(root, range) {
                actions.push(action);
            }
            if let Some(action) = self.convert_to_named_function(root, range) {
                actions.push(action);
            }
            if let Some(action) = self.inline_variable(root, range) {
                actions.push(action);
            }
            actions.extend(self.generate_accessors(root, range));
            if let Some(action) = self.convert_namespace_to_named(root, range) {
                actions.push(action);
            }
            if let Some(action) = self.convert_named_to_namespace(root, range) {
                actions.push(action);
            }
            if let Some(action) = self.sort_import_specifiers(root, range) {
                actions.push(action);
            }

            // Template string conversions
            if let Some(action) = self.convert_to_template_string(root, range) {
                actions.push(action);
            }
            if let Some(action) = self.convert_to_string_concatenation(root, range) {
                actions.push(action);
            }

            // Arrow function braces
            if let Some(action) = self.add_braces_to_arrow_function(root, range) {
                actions.push(action);
            }
            if let Some(action) = self.remove_braces_from_arrow_function(root, range) {
                actions.push(action);
            }

            // Optional chaining
            if let Some(action) = self.convert_to_optional_chaining(root, range) {
                actions.push(action);
            }

            // Nullish coalescing
            if let Some(action) = self.convert_to_nullish_coalescing(root, range) {
                actions.push(action);
            }

            // Move to new file
            if let Some(action) = self.move_to_new_file(root, range) {
                actions.push(action);
            }

            // Extract interface from class
            if let Some(action) = self.extract_interface_from_class(root, range) {
                actions.push(action);
            }

            // Convert parameters to destructured object
            if let Some(action) = self.convert_params_to_destructured(root, range) {
                actions.push(action);
            }

            // Add return type
            if let Some(action) = self.add_return_type(root, range) {
                actions.push(action);
            }

            // Convert to async/await
            if let Some(action) = self.convert_to_async_await(root, range) {
                actions.push(action);
            }

            // Convert between named and default exports
            if let Some(action) = self.convert_to_default_export(root, range) {
                actions.push(action);
            }
            if let Some(action) = self.convert_to_named_export(root, range) {
                actions.push(action);
            }
        }

        // Quick fixes requiring deeper analysis
        if request_quickfix {
            if let Some(action) = self.implement_interface(root, range) {
                actions.push(action);
            }
            if let Some(action) = self.override_methods(root, range) {
                actions.push(action);
            }
            if let Some(action) = self.add_missing_switch_cases(root, range) {
                actions.push(action);
            }
            actions.extend(self.fix_all_actions(root, &context.diagnostics));
        }

        actions
    }

    // -------------------------------------------------------------------------
    // Unused import quick fix
    // -------------------------------------------------------------------------

    fn unused_import_quickfix(&self, diag: &LspDiagnostic) -> Option<CodeAction> {
        let code = diag.code?;
        if code != tsz_checker::diagnostics::diagnostic_codes::ALL_IMPORTS_IN_IMPORT_DECLARATION_ARE_UNUSED {
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

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), vec![edit]);

        Some(CodeAction {
            title,
            kind: CodeActionKind::QuickFix,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: true,
            data: Some(serde_json::json!({
                "fixName": "unusedIdentifier",
                "fixId": "unusedIdentifier_delete",
                "fixAllDescription": "Delete all unused declarations"
            })),
        })
    }

    // -------------------------------------------------------------------------
    // Unused declaration quick fix
    // -------------------------------------------------------------------------

    fn unused_declaration_quickfix(&self, diag: &LspDiagnostic) -> Option<CodeAction> {
        let code = diag.code?;
        if code != tsz_checker::diagnostics::diagnostic_codes::ALL_VARIABLES_ARE_UNUSED {
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

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), vec![edit]);

        let title = format!("Remove unused declaration '{name}'");

        Some(CodeAction {
            title,
            kind: CodeActionKind::QuickFix,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: true,
            data: Some(serde_json::json!({
                "fixName": "unusedIdentifier",
                "fixId": "unusedIdentifier_delete",
                "fixAllDescription": "Delete all unused declarations"
            })),
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
    pub(super) fn declaration_removal_range(
        &self,
        node: &tsz_parser::parser::node::Node,
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

    // -------------------------------------------------------------------------
    // Missing property quick fix
    // -------------------------------------------------------------------------

    fn missing_property_quickfix(&self, diag: &LspDiagnostic) -> Option<CodeAction> {
        let code = diag.code?;
        if code != tsz_checker::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE {
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

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), edits);

        Some(CodeAction {
            title,
            kind: CodeActionKind::QuickFix,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
            data: Some(serde_json::json!({
                "fixName": "fixMissingMember",
                "fixId": "fixMissingMember",
                "fixAllDescription": "Add all missing members"
            })),
        })
    }

    fn add_missing_const_quickfix(&self, diag: &LspDiagnostic) -> Option<CodeAction> {
        let code = diag.code?;
        if code != 2304 && code != 18004 {
            return None;
        }

        let start_offset = self
            .line_map
            .position_to_offset(diag.range.start, self.source)? as usize;
        let (line_start, line_end) = self.line_bounds(start_offset)?;
        let line = self.source.get(line_start..line_end)?;
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            return None;
        }
        if trimmed.starts_with("const ")
            || trimmed.starts_with("let ")
            || trimmed.starts_with("var ")
            || trimmed.starts_with("import ")
            || trimmed.starts_with("export ")
            || trimmed.starts_with("function ")
            || trimmed.starts_with("class ")
            || trimmed.starts_with("interface ")
            || trimmed.starts_with("type ")
            || trimmed.starts_with("enum ")
        {
            return None;
        }

        let insertion_offset = if (trimmed.starts_with("for (")
            || trimmed.starts_with("for await ("))
            && (trimmed.contains(" in ") || trimmed.contains(" of "))
        {
            let open_idx = line.find('(')?;
            let after_open = line.get(open_idx + 1..)?.trim_start();
            if after_open.starts_with("const ")
                || after_open.starts_with("let ")
                || after_open.starts_with("var ")
            {
                return None;
            }
            line_start + open_idx + 1
        } else {
            let starts_with_target = trimmed.chars().next().is_some_and(|ch| {
                ch.is_ascii_alphabetic() || ch == '_' || ch == '$' || ch == '[' || ch == '{'
            });
            if !starts_with_target || !trimmed.contains('=') {
                return None;
            }
            line_start + line.len().saturating_sub(trimmed.len())
        };

        let insert_pos = self
            .line_map
            .offset_to_position(insertion_offset as u32, self.source);
        let edit = TextEdit {
            range: Range::new(insert_pos, insert_pos),
            new_text: "const ".to_string(),
        };

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), vec![edit]);

        Some(CodeAction {
            title: "Add 'const' to unresolved variable".to_string(),
            kind: CodeActionKind::QuickFix,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
            data: Some(serde_json::json!({
                "fixName": "addMissingConst",
                "fixId": "addMissingConst",
                "fixAllDescription": "Add 'const' to all unresolved variables"
            })),
        })
    }

    fn line_bounds(&self, offset: usize) -> Option<(usize, usize)> {
        if offset > self.source.len() {
            return None;
        }
        let prefix = self.source.get(..offset)?;
        let line_start = prefix.rfind('\n').map_or(0, |idx| idx + 1);
        let suffix = self.source.get(offset..)?;
        let rel_end = suffix.find('\n').unwrap_or(suffix.len());
        Some((line_start, offset + rel_end))
    }

    // -------------------------------------------------------------------------
    // Property access and edit helpers
    // -------------------------------------------------------------------------

    fn property_access_info(&self, node_idx: NodeIndex) -> Option<PropertyAccessInfo> {
        let mut current = node_idx;
        while current.is_some() {
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
        object_node: &tsz_parser::parser::node::Node,
        literal: &tsz_parser::parser::node::LiteralExprData,
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
            let prefix = if elements.is_empty() || had_trailing_comma {
                " "
            } else {
                ", "
            };
            let mut new_text = format!("{prefix}{property_text}: undefined");
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
            format!("{close_indent}{indent_unit}")
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
            let new_text = format!(" {property_text}: any;{trailing_space}");
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
            format!("{close_indent}{indent_unit}")
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
        while current.is_some() {
            let node = self.arena.get(current)?;
            if node.is_class_like() {
                return Some(current);
            }
            current = self.arena.get_extended(current)?.parent;
        }
        None
    }

    // -------------------------------------------------------------------------
    // Shared utilities (used by both this file and code_action_imports.rs)
    // -------------------------------------------------------------------------

    pub(super) fn find_closing_brace_offset(
        &self,
        node: &tsz_parser::parser::node::Node,
    ) -> Option<u32> {
        let slice = self.source.get(node.pos as usize..node.end as usize)?;
        let rel = slice.rfind('}')?;
        Some(node.pos + rel as u32)
    }

    pub(super) fn indent_at_offset(&self, offset: u32) -> String {
        let pos = self.line_map.offset_to_position(offset, self.source);
        self.get_indentation_at_position(&Position::new(pos.line, 0))
    }

    pub(super) fn indent_unit_from(&self, base_indent: &str) -> &str {
        if base_indent.contains('\t') {
            "\t"
        } else {
            "  "
        }
    }

    /// Get the indentation (leading whitespace) at a given position.
    pub(super) fn get_indentation_at_position(&self, pos: &Position) -> String {
        let line_start = self.line_map.line_start(pos.line as usize).unwrap_or(0);
        let slice = self.source.get(line_start as usize..).unwrap_or("");
        slice
            .chars()
            .take_while(|ch| *ch == ' ' || *ch == '\t')
            .collect()
    }
}

struct PropertyAccessInfo {
    access_node: NodeIndex,
    target: NodeIndex,
    property_name: String,
    property_text: String,
}
