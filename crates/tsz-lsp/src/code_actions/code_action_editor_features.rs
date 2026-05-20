//! Editor features support for LSP.
//!
//! - Workspace `executeCommand` support
//! - Source actions on save (organize imports, add missing imports, remove unused, sort)
//! - Go to file references
//! - Paste with imports detection
//! - Workspace edit support for willCreate/willDelete

use crate::rename::{TextEdit, WorkspaceEdit};
use rustc_hash::FxHashMap;
use tsz_parser::parser::node::NodeAccess;

use super::code_action_provider::{CodeAction, CodeActionKind, CodeActionProvider};
use tsz_common::position::Range;

/// Custom command identifiers that the LSP server can register.
pub struct LspCommands;

impl LspCommands {
    pub const ORGANIZE_IMPORTS: &'static str = "_typescript.organizeImports";
    pub const APPLY_WORKSPACE_EDIT: &'static str = "_typescript.applyWorkspaceEdit";
    pub const ADD_MISSING_IMPORTS: &'static str = "_typescript.addMissingImports";
    pub const REMOVE_UNUSED_IMPORTS: &'static str = "_typescript.removeUnusedImports";
    pub const SORT_IMPORTS: &'static str = "_typescript.sortImports";
    pub const FIX_ALL: &'static str = "_typescript.fixAll";

    /// Return all registered command identifiers.
    pub fn all() -> Vec<&'static str> {
        vec![
            Self::ORGANIZE_IMPORTS,
            Self::APPLY_WORKSPACE_EDIT,
            Self::ADD_MISSING_IMPORTS,
            Self::REMOVE_UNUSED_IMPORTS,
            Self::SORT_IMPORTS,
            Self::FIX_ALL,
        ]
    }
}

/// Source action kinds that can be registered in server capabilities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceActionKind {
    OrganizeImports,
    AddMissingImports,
    RemoveUnusedImports,
    SortImports,
}

impl SourceActionKind {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::OrganizeImports => "source.organizeImports",
            Self::AddMissingImports => "source.addMissingImports",
            Self::RemoveUnusedImports => "source.removeUnusedImports",
            Self::SortImports => "source.sortImports",
        }
    }

    /// All supported source action kinds.
    pub fn all() -> Vec<Self> {
        vec![
            Self::OrganizeImports,
            Self::AddMissingImports,
            Self::RemoveUnusedImports,
            Self::SortImports,
        ]
    }
}

/// File reference information for "Go to file references".
#[derive(Debug, Clone)]
pub struct FileReference {
    /// The file that contains the import.
    pub referencing_file: String,
    /// The import statement range in the referencing file.
    pub import_range: Range,
    /// The module specifier used in the import.
    pub module_specifier: String,
}

/// Information about a file event (create/delete) for workspace edits.
#[derive(Debug, Clone)]
pub struct FileEvent {
    pub uri: String,
    pub kind: FileEventKind,
}

#[derive(Debug, Clone)]
pub enum FileEventKind {
    Created,
    Deleted,
}

/// Result of analyzing pasted code for missing imports.
#[derive(Debug, Clone)]
pub struct PasteAnalysis {
    /// Identifiers found in the pasted code that may need imports.
    pub unresolved_identifiers: Vec<String>,
    /// The range where the code was pasted.
    pub paste_range: Range,
}

impl<'a> CodeActionProvider<'a> {
    /// Generate source action code actions (for on-save triggers).
    ///
    /// Returns actions for:
    /// - `source.removeUnusedImports` — remove unused imports only
    /// - `source.sortImports` — sort import declarations only
    ///
    /// `source.addMissingImports` is intentionally NOT advertised: it would
    /// need workspace-level import candidate resolution that this LSP does
    /// not yet implement, and previously surfaced as an editor entry that
    /// did nothing (`edit: None`). See robustness audit
    /// `docs/architecture/ROBUSTNESS_AUDIT_2026-04-26.md` item 16 (PR #P).
    /// When the candidate-resolution path is added, advertise this action
    /// only when at least one candidate is found.
    ///
    /// Note: `source.organizeImports` is handled separately in `organize_imports()`.
    pub fn source_actions(&self, root: tsz_parser::NodeIndex) -> Vec<CodeAction> {
        let mut actions = Vec::new();

        // Remove unused imports action
        if let Some(action) = self.remove_unused_imports_action(root) {
            actions.push(action);
        }

        // Sort imports action
        if let Some(action) = self.sort_imports_action(root) {
            actions.push(action);
        }

        actions
    }

    /// Remove unused imports source action.
    ///
    /// Scans all imports and removes any that the binder marks as unused.
    fn remove_unused_imports_action(&self, root: tsz_parser::NodeIndex) -> Option<CodeAction> {
        let root_node = self.arena.get(root)?;
        let source_data = self.arena.get_source_file(root_node)?;

        let mut edits = Vec::new();

        for &stmt_idx in &source_data.statements.nodes {
            let stmt_node = self.arena.get(stmt_idx)?;
            if stmt_node.kind != tsz_parser::syntax_kind_ext::IMPORT_DECLARATION {
                continue;
            }

            let import_data = self.arena.get_import_decl(stmt_node)?;
            if import_data.import_clause.is_none() {
                continue; // Side-effect import, keep it
            }

            let clause_node = self.arena.get(import_data.import_clause)?;
            let clause = self.arena.get_import_clause(clause_node)?;

            // Check if default import is unused
            let default_unused = if clause.name.is_some() {
                self.is_identifier_unused(clause.name)
            } else {
                true // no default import
            };

            // Check if namespace/named bindings are unused
            let bindings_unused = if clause.named_bindings.is_some() {
                let bindings_node = self.arena.get(clause.named_bindings)?;
                if bindings_node.kind == tsz_parser::syntax_kind_ext::NAMESPACE_IMPORT {
                    let named = self.arena.get_named_imports(bindings_node);
                    named
                        .map(|n| self.is_identifier_unused(n.name))
                        .unwrap_or(true)
                } else if bindings_node.kind == tsz_parser::syntax_kind_ext::NAMED_IMPORTS {
                    let named = self.arena.get_named_imports(bindings_node);
                    named
                        .map(|n| {
                            n.elements
                                .nodes
                                .iter()
                                .all(|&spec| self.is_import_specifier_unused(spec))
                        })
                        .unwrap_or(true)
                } else {
                    true
                }
            } else {
                true // no bindings
            };

            if default_unused && bindings_unused {
                // Remove the entire import
                let (range, _) = self.declaration_removal_range(stmt_node);
                edits.push(TextEdit {
                    range,
                    new_text: String::new(),
                });
            }
        }

        if edits.is_empty() {
            return None;
        }

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), edits);

        Some(CodeAction {
            title: "Remove Unused Imports".to_string(),
            kind: CodeActionKind::SourceRemoveUnusedImports,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
            data: None,
        })
    }

    /// Sort import declarations source action.
    ///
    /// Sorts import declarations by module specifier without removing any.
    fn sort_imports_action(&self, root: tsz_parser::NodeIndex) -> Option<CodeAction> {
        // Delegate to organize_imports which already sorts
        let action = self.organize_imports(root)?;

        Some(CodeAction {
            title: "Sort Imports".to_string(),
            kind: CodeActionKind::SourceSortImports,
            edit: action.edit,
            is_preferred: false,
            data: None,
        })
    }

    /// Check if an identifier node appears to be unused.
    ///
    /// Uses the binder's symbol resolution to check if a symbol has references.
    fn is_identifier_unused(&self, name_idx: tsz_parser::NodeIndex) -> bool {
        if name_idx.is_none() {
            return true;
        }

        let Some(sym_id) = self.binder.resolve_identifier(self.arena, name_idx) else {
            return true;
        };
        let Some(symbol) = self.binder.symbols.get(sym_id) else {
            return true;
        };

        // If the symbol has only 1 declaration (the import itself) and no references,
        // it's likely unused. This is a heuristic — full unused detection requires
        // cross-file analysis from the checker.
        symbol.declarations.len() <= 1
    }

    /// Check if an import specifier is unused.
    fn is_import_specifier_unused(&self, spec_idx: tsz_parser::NodeIndex) -> bool {
        let Some(spec_node) = self.arena.get(spec_idx) else {
            return true;
        };
        if spec_node.kind != tsz_parser::syntax_kind_ext::IMPORT_SPECIFIER {
            return true;
        }
        let Some(spec_data) = self.arena.get_specifier(spec_node) else {
            return true;
        };
        // The local name is what matters for usage
        let local_name = if spec_data.name.is_some() {
            spec_data.name
        } else {
            spec_data.property_name
        };
        self.is_identifier_unused(local_name)
    }

    /// Find all files that import the current file.
    ///
    /// This is a single-file operation that returns import module specifiers
    /// from the current file. The full "go to file references" feature requires
    /// workspace-level coordination at the LSP server layer to reverse the
    /// relationship (finding files that import *this* file).
    pub fn collect_import_specifiers(&self, root: tsz_parser::NodeIndex) -> Vec<String> {
        let mut specifiers = Vec::new();

        let Some(source_node) = self.arena.get(root) else {
            return specifiers;
        };
        let Some(source_data) = self.arena.get_source_file(source_node) else {
            return specifiers;
        };

        for &stmt_idx in &source_data.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind == tsz_parser::syntax_kind_ext::IMPORT_DECLARATION
                && let Some(import_data) = self.arena.get_import_decl(stmt_node)
                && let Some(spec_node) = self.arena.get(import_data.module_specifier)
                && let Some(text) = self
                    .source
                    .get(spec_node.pos as usize..spec_node.end as usize)
            {
                let trimmed = text.trim_matches(|c| c == '\'' || c == '"');
                specifiers.push(trimmed.to_string());
            }
        }

        specifiers
    }

    /// Analyze pasted code for identifiers that may need imports.
    ///
    /// Given a range of pasted text, extract identifiers that are unresolved
    /// in the current scope. The LSP server layer can then use import candidates
    /// to add the necessary imports.
    pub fn analyze_paste_for_imports(
        &self,
        root: tsz_parser::NodeIndex,
        paste_range: Range,
    ) -> PasteAnalysis {
        let mut unresolved = Vec::new();

        let start = self
            .line_map
            .position_to_offset(paste_range.start, self.source)
            .unwrap_or(0);
        let end = self
            .line_map
            .position_to_offset(paste_range.end, self.source)
            .unwrap_or(0);

        // Scan for capitalized identifiers in the pasted range that look like type/class refs
        if let Some(text) = self.source.get(start as usize..end as usize) {
            for word in text.split(|c: char| !c.is_alphanumeric() && c != '_') {
                if word.is_empty() {
                    continue;
                }
                let first = word.chars().next().unwrap();
                if first.is_uppercase() {
                    // Looks like a type/class reference - check if it's resolved
                    let _ = root; // root available for deeper analysis if needed
                    if !self.is_name_in_scope(root, word) {
                        unresolved.push(word.to_string());
                    }
                }
            }
        }

        // Deduplicate
        unresolved.sort();
        unresolved.dedup();

        PasteAnalysis {
            unresolved_identifiers: unresolved,
            paste_range,
        }
    }

    /// Check if a name is already defined in the current file scope.
    fn is_name_in_scope(&self, root: tsz_parser::NodeIndex, name: &str) -> bool {
        let Some(source_node) = self.arena.get(root) else {
            return false;
        };
        let Some(source_data) = self.arena.get_source_file(source_node) else {
            return false;
        };

        // Check imports
        for &stmt_idx in &source_data.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind == tsz_parser::syntax_kind_ext::IMPORT_DECLARATION
                && let Some(import_data) = self.arena.get_import_decl(stmt_node)
                && let Some(clause_node) = self.arena.get(import_data.import_clause)
                && let Some(clause) = self.arena.get_import_clause(clause_node)
            {
                // Check default import name
                if let Some(n) = self.arena.get_identifier_text(clause.name)
                    && n == name
                {
                    return true;
                }
            }

            // Check top-level declarations
            match stmt_node.kind {
                k if k == tsz_parser::syntax_kind_ext::FUNCTION_DECLARATION => {
                    if let Some(func) = self.arena.get_function(stmt_node)
                        && self.arena.get_identifier_text(func.name) == Some(name)
                    {
                        return true;
                    }
                }
                k if k == tsz_parser::syntax_kind_ext::CLASS_DECLARATION => {
                    if let Some(class) = self.arena.get_class(stmt_node)
                        && self.arena.get_identifier_text(class.name) == Some(name)
                    {
                        return true;
                    }
                }
                k if k == tsz_parser::syntax_kind_ext::INTERFACE_DECLARATION => {
                    if let Some(iface) = self.arena.get_interface(stmt_node)
                        && self.arena.get_identifier_text(iface.name) == Some(name)
                    {
                        return true;
                    }
                }
                k if k == tsz_parser::syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                    if let Some(alias) = self.arena.get_type_alias(stmt_node)
                        && self.arena.get_identifier_text(alias.name) == Some(name)
                    {
                        return true;
                    }
                }
                k if k == tsz_parser::syntax_kind_ext::ENUM_DECLARATION => {
                    if let Some(e) = self.arena.get_enum(stmt_node)
                        && self.arena.get_identifier_text(e.name) == Some(name)
                    {
                        return true;
                    }
                }
                _ => {}
            }
        }

        false
    }

    /// Generate import cleanup edits for a file delete event.
    ///
    /// When a file is deleted, imports pointing to that file should be removed.
    /// Returns edits to remove those imports from the current file.
    pub fn handle_file_deleted(
        &self,
        root: tsz_parser::NodeIndex,
        deleted_specifier: &str,
    ) -> Option<WorkspaceEdit> {
        let root_node = self.arena.get(root)?;
        let source_data = self.arena.get_source_file(root_node)?;

        let mut edits = Vec::new();

        for &stmt_idx in &source_data.statements.nodes {
            let stmt_node = self.arena.get(stmt_idx)?;
            if stmt_node.kind != tsz_parser::syntax_kind_ext::IMPORT_DECLARATION {
                continue;
            }

            let import_data = self.arena.get_import_decl(stmt_node)?;
            if let Some(spec_node) = self.arena.get(import_data.module_specifier)
                && let Some(spec_text) = self
                    .source
                    .get(spec_node.pos as usize..spec_node.end as usize)
            {
                let trimmed = spec_text.trim_matches(|c| c == '\'' || c == '"');
                if trimmed == deleted_specifier {
                    let (range, _) = self.declaration_removal_range(stmt_node);
                    edits.push(TextEdit {
                        range,
                        new_text: String::new(),
                    });
                }
            }
        }

        if edits.is_empty() {
            return None;
        }

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), edits);

        Some(WorkspaceEdit { changes })
    }
}
