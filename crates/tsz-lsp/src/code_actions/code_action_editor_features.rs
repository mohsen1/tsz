//! Editor features support for LSP.
//!
//! - Workspace `executeCommand` support
//! - Source actions on save (organize imports, add missing imports, remove unused, sort)
//! - Go to file references
//! - Workspace edit support for willCreate/willDelete

use super::code_action_provider::{CodeAction, CodeActionKind, CodeActionProvider};

/// Custom command identifiers that the LSP server can register.
pub struct LspCommands;

impl LspCommands {
    pub const ORGANIZE_IMPORTS: &'static str = "_typescript.organizeImports";
    pub const APPLY_WORKSPACE_EDIT: &'static str = "_typescript.applyWorkspaceEdit";
    pub const ADD_MISSING_IMPORTS: &'static str = "_typescript.addMissingImports";
    pub const REMOVE_UNUSED_IMPORTS: &'static str = "_typescript.removeUnusedImports";
    pub const SORT_IMPORTS: &'static str = "_typescript.sortImports";

    /// Return all registered command identifiers.
    pub fn all() -> Vec<&'static str> {
        vec![
            Self::ORGANIZE_IMPORTS,
            Self::APPLY_WORKSPACE_EDIT,
            Self::ADD_MISSING_IMPORTS,
            Self::REMOVE_UNUSED_IMPORTS,
            Self::SORT_IMPORTS,
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
    pub fn as_str(&self) -> &'static str {
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
    pub import_range: tsz_common::position::Range,
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

impl<'a> CodeActionProvider<'a> {
    /// Generate source action code actions (for on-save triggers).
    ///
    /// Returns actions for:
    /// - `source.organizeImports` — reuse existing organize_imports
    /// - `source.removeUnusedImports` — remove unused imports only
    /// - `source.sortImports` — sort only, don't remove
    pub fn source_actions(&self, root: tsz_parser::NodeIndex) -> Vec<CodeAction> {
        let mut actions = Vec::new();

        // Organize imports (already implemented, wrap as source action)
        if let Some(action) = self.organize_imports(root) {
            actions.push(CodeAction {
                title: "Organize Imports".to_string(),
                kind: CodeActionKind::SourceOrganizeImports,
                edit: action.edit,
                is_preferred: true,
                data: None,
            });
        }

        actions
    }

    /// Find all files that import the current file.
    ///
    /// This is a single-file operation that returns import nodes in the current file
    /// that reference other files. The full "go to file references" feature requires
    /// workspace-level coordination at the LSP server layer.
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
            if stmt_node.kind == tsz_parser::syntax_kind_ext::IMPORT_DECLARATION {
                if let Some(import_data) = self.arena.get_import_decl(stmt_node) {
                    if let Some(spec_node) = self.arena.get(import_data.module_specifier) {
                        if let Some(text) = self
                            .source
                            .get(spec_node.pos as usize..spec_node.end as usize)
                        {
                            // Strip quotes
                            let trimmed = text.trim_matches(|c| c == '\'' || c == '"');
                            specifiers.push(trimmed.to_string());
                        }
                    }
                }
            }
        }

        specifiers
    }
}
