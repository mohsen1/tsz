//! File rename handling for LSP.
//!
//! Provides support for `workspace/willRenameFiles` to update import statements
//! when files are renamed or moved.

use tsz_common::position::{Position, Range};
use tsz_parser::{NodeIndex, syntax_kind_ext};

use crate::rename::TextEdit;
use crate::utils::{is_import_keyword, is_require_identifier};

/// Information about an import/export that needs to be updated.
#[derive(Debug, Clone)]
pub struct ImportLocation {
    /// The node containing the module specifier string
    pub specifier_node: NodeIndex,
    /// Full range of the string literal, **including the surrounding quotes**.
    ///
    /// Callers rewriting the specifier should prefer [`Self::specifier_text_edit`]
    /// so the quote characters are preserved without needing to re-detect the
    /// original quote style.
    pub range: Range,
    /// The current specifier value with the quotes stripped (e.g. `./utils`).
    pub current_specifier: String,
}

impl ImportLocation {
    /// Build a `TextEdit` that replaces the specifier content between the
    /// surrounding quotes with `new_specifier`. The original quote characters
    /// are untouched, so the rewrite preserves the file's quote style without
    /// the caller having to recreate them.
    pub const fn specifier_text_edit(&self, new_specifier: String) -> TextEdit {
        let inner = Range::new(
            Position::new(self.range.start.line, self.range.start.character + 1),
            Position::new(
                self.range.end.line,
                self.range.end.character.saturating_sub(1),
            ),
        );
        TextEdit::new(inner, new_specifier)
    }
}

define_lsp_provider!(minimal FileRenameProvider, "Provider for finding imports/exports that reference a renamed file.");

impl<'a> FileRenameProvider<'a> {
    /// Find all import/export specifiers in the given AST that might reference
    /// the renamed file.
    ///
    /// # Arguments
    /// * `root` - The root node of the AST (typically `SourceFile`)
    /// * `target_path` - The path of the file being renamed (for filtering)
    ///
    /// # Returns
    /// A list of all import/export locations found in this file
    ///
    /// # Note
    /// This returns ALL imports/exports in the file. The caller is responsible
    /// for filtering to only those that actually reference the renamed file,
    /// since that requires knowing the module resolution context.
    pub fn find_import_specifier_nodes(&self, _root: NodeIndex) -> Vec<ImportLocation> {
        let mut result = Vec::new();

        // In the flat NodeArena structure, we do a simple linear scan of all nodes
        // This is efficient because NodeArena is contiguous in memory
        for (i, node) in self.arena.nodes.iter().enumerate() {
            let node_idx = NodeIndex(i as u32);
            // Check if this is an import or export declaration
            if node.kind == syntax_kind_ext::IMPORT_DECLARATION {
                if let Some(import_decl) = self.arena.get_import_decl(node) {
                    self.add_import_location(import_decl.module_specifier, &mut result);
                }
            } else if node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                if let Some(export_decl) = self.arena.get_export_decl(node) {
                    self.add_import_location(export_decl.module_specifier, &mut result);
                }
            } else if node.kind == syntax_kind_ext::CALL_EXPRESSION {
                // Check for dynamic imports: import("./module") or require("./module")
                self.try_add_call_expression(node_idx, &mut result);
            }
        }

        result
    }

    /// Try to add an import location from a call expression (dynamic import or require).
    fn try_add_call_expression(&self, call_idx: NodeIndex, result: &mut Vec<ImportLocation>) {
        let Some(call_node) = self.arena.get(call_idx) else {
            return;
        };

        let Some(call_data) = self.arena.get_call_expr(call_node) else {
            return;
        };

        let is_dynamic_import = is_import_keyword(self.arena, call_data.expression);
        let is_require = is_require_identifier(self.arena, call_data.expression);

        if !is_dynamic_import && !is_require {
            return;
        }

        // Get the first argument, which should be the module specifier string
        let Some(args) = &call_data.arguments else {
            return;
        };

        let Some(&first_arg) = args.nodes.first() else {
            return;
        };

        self.add_import_location(first_arg, result);
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
        let content = if (text.starts_with('"') && text.ends_with('"') && text.len() > 1)
            || (text.starts_with('\'') && text.ends_with('\'') && text.len() > 1)
        {
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
