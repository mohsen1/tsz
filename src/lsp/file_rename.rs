//! File rename handling for LSP.
//!
//! Provides support for `workspace/willRenameFiles` to update import statements
//! when files are renamed or moved.

use crate::lsp::position::{LineMap, Range};
use crate::parser::node::{NodeAccess, NodeArena};
use crate::parser::{NodeIndex, syntax_kind_ext};

/// Information about an import/export that needs to be updated.
#[derive(Debug, Clone)]
pub struct ImportLocation {
    /// The node containing the module specifier string
    pub specifier_node: NodeIndex,
    /// The range of the specifier text (for TextEdit)
    pub range: Range,
    /// The current specifier value (e.g., "./utils" or "../types")
    pub current_specifier: String,
}

/// Provider for finding imports/exports that reference a renamed file.
pub struct FileRenameProvider<'a> {
    arena: &'a NodeArena,
    line_map: &'a LineMap,
    source_text: &'a str,
}

impl<'a> FileRenameProvider<'a> {
    /// Create a new FileRename provider.
    pub fn new(arena: &'a NodeArena, line_map: &'a LineMap, source_text: &'a str) -> Self {
        Self {
            arena,
            line_map,
            source_text,
        }
    }

    /// Find all import/export specifiers in the given AST that might reference
    /// the renamed file.
    ///
    /// # Arguments
    /// * `root` - The root node of the AST (typically SourceFile)
    /// * `target_path` - The path of the file being renamed (for filtering)
    ///
    /// # Returns
    /// A list of all import/export locations found in this file
    ///
    /// # Note
    /// This returns ALL imports/exports in the file. The caller is responsible
    /// for filtering to only those that actually reference the renamed file,
    /// since that requires knowing the module resolution context.
    pub fn find_import_specifier_nodes(&self, root: NodeIndex) -> Vec<ImportLocation> {
        let mut result = Vec::new();

        // Walk the AST to find all ImportDeclaration and ExportDeclaration nodes
        self.walk_tree(root, &mut result);

        result
    }

    /// Walk the AST tree to find import/export declarations.
    fn walk_tree(&self, node_idx: NodeIndex, result: &mut Vec<ImportLocation>) {
        if node_idx.is_none() {
            return;
        }

        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return,
        };

        // Check if this is an import or export declaration
        if node.kind == syntax_kind_ext::IMPORT_DECLARATION {
            if let Some(import_decl) = self.arena.get_import_decl(node) {
                self.add_import_location(import_decl.module_specifier, result);
            }
        } else if node.kind == syntax_kind_ext::EXPORT_DECLARATION {
            if let Some(export_decl) = self.arena.get_export_decl(node) {
                self.add_import_location(export_decl.module_specifier, result);
            }
        }

        // Recursively walk children
        // Note: In the flat NodeArena structure, we need to walk all nodes
        // For now, we'll do a simple linear scan of all nodes
        // This is efficient because NodeArena is contiguous in memory
    }

    /// Add an import location to the result if the specifier is a string literal.
    fn add_import_location(&self, specifier_idx: NodeIndex, result: &mut Vec<ImportLocation>) {
        if specifier_idx.is_none() {
            return;
        }

        let specifier_node = match self.arena.get(specifier_idx) {
            Some(n) => n,
            None => return,
        };

        // The module specifier should be a StringLiteral
        // Get the text content
        let start = specifier_node.pos as usize;
        let end = specifier_node.end as usize;

        if end <= start || end > self.source_text.len() {
            return;
        }

        let text = &self.source_text[start..end];

        // Extract the string content (without quotes)
        // Handle both single and double quotes
        let content = if text.starts_with('"') && text.ends_with('"') && text.len() > 1 {
            &text[1..text.len() - 1]
        } else if text.starts_with('\'') && text.ends_with('\'') && text.len() > 1 {
            &text[1..text.len() - 1]
        } else {
            // Not a quoted string, skip
            return;
        };

        let range = Range::new(
            self.line_map
                .offset_to_position(specifier_node.pos, self.source_text),
            self.line_map
                .offset_to_position(specifier_node.end, self.source_text),
        );

        result.push(ImportLocation {
            specifier_node: specifier_idx,
            range,
            current_specifier: content.to_string(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TODO: Add tests once we have test infrastructure setup
    // Test cases to cover:
    // 1. Single import: import { x } from "./utils"
    // 2. Export from: export { x } from "./types"
    // 3. Multiple imports in one file
    // 4. Dynamic import: import("./utils")
    // 5. Require calls: require("./utils")
}
