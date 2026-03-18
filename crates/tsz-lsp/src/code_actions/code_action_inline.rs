//! Inline Variable refactoring for the LSP.
//!
//! Replaces all references to a variable with its initializer expression,
//! then removes the variable declaration.

use crate::rename::{TextEdit, WorkspaceEdit};
use crate::resolver::ScopeWalker;
use crate::utils::find_node_at_offset;
use rustc_hash::FxHashMap;
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

use super::code_action_provider::{CodeAction, CodeActionKind, CodeActionProvider};
use tsz_common::position::Range;

impl<'a> CodeActionProvider<'a> {
    /// Inline a variable: replace all references with the initializer expression
    /// and remove the declaration.
    pub fn inline_variable(&self, root: NodeIndex, range: Range) -> Option<CodeAction> {
        // 1. Convert position to offset and find node
        let start_offset = self.line_map.position_to_offset(range.start, self.source)?;
        let node_idx = find_node_at_offset(self.arena, start_offset);
        if node_idx.is_none() {
            return None;
        }

        // The node should be an identifier
        let node = self.arena.get(node_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let var_name = self.arena.get_identifier_text(node_idx)?.to_string();

        // 2. Walk up to find VARIABLE_DECLARATION
        let var_decl_idx = self.find_ancestor_variable_declaration(node_idx)?;
        let var_decl_node = self.arena.get(var_decl_idx)?;
        let var_decl = self.arena.get_variable_declaration(var_decl_node)?;

        // 3. Must have an initializer
        if var_decl.initializer.is_none() {
            return None;
        }

        // 4. Get the initializer text
        let init_node = self.arena.get(var_decl.initializer)?;
        let init_text = self
            .source
            .get(init_node.pos as usize..init_node.end as usize)?
            .to_string();

        // 5. Resolve the symbol for this variable
        let symbol_id = self.binder.resolve_identifier(self.arena, var_decl.name)?;

        // 6. Find all references using ScopeWalker
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let references = walker.find_references(root, symbol_id);

        // Separate declaration references from usage references.
        // The declaration name node itself is a reference we want to skip
        // (we remove it via the statement deletion edit).
        let usage_refs: Vec<NodeIndex> = references
            .into_iter()
            .filter(|&r| r != var_decl.name)
            .collect();

        // Must have at least one usage to inline
        if usage_refs.is_empty() {
            return None;
        }

        // 7. Build TextEdits
        let mut edits = Vec::new();

        // 7a. One edit per reference to replace with initializer text
        for &ref_idx in &usage_refs {
            let ref_node = self.arena.get(ref_idx)?;
            let start = self.line_map.offset_to_position(ref_node.pos, self.source);
            let end = self.line_map.offset_to_position(ref_node.end, self.source);
            edits.push(TextEdit {
                range: Range::new(start, end),
                new_text: init_text.clone(),
            });
        }

        // 7b. Delete the variable declaration statement.
        // Walk up from VARIABLE_DECLARATION to find the containing statement
        // (VARIABLE_STATEMENT or VARIABLE_DECLARATION_LIST).
        let removal_range = self.inline_variable_removal_range(var_decl_idx)?;
        edits.push(TextEdit {
            range: removal_range,
            new_text: String::new(),
        });

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), edits);

        Some(CodeAction {
            title: format!("Inline variable '{var_name}'"),
            kind: CodeActionKind::RefactorInline,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
            data: None,
        })
    }

    /// Walk up from a node to find an ancestor `VARIABLE_DECLARATION`.
    fn find_ancestor_variable_declaration(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = node_idx;
        for _ in 0..10 {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                return Some(current);
            }
            let ext = self.arena.get_extended(current)?;
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }
        None
    }

    /// Compute the range to delete when inlining a variable declaration.
    ///
    /// If the declaration list has only one declaration, remove the entire
    /// `VARIABLE_STATEMENT` (including `const`/`let`/`var` keyword, semicolon,
    /// and trailing newline). If there are multiple declarations in the list,
    /// remove only the single declaration (and its separator comma).
    fn inline_variable_removal_range(&self, var_decl_idx: NodeIndex) -> Option<Range> {
        // Walk up: VARIABLE_DECLARATION -> VARIABLE_DECLARATION_LIST -> VARIABLE_STATEMENT
        let ext = self.arena.get_extended(var_decl_idx)?;
        let list_idx = ext.parent;
        if list_idx.is_none() {
            return None;
        }
        let list_node = self.arena.get(list_idx)?;
        if list_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            return None;
        }
        let list_data = self.arena.get_variable(list_node)?;
        let decl_count = list_data.declarations.nodes.len();

        if decl_count <= 1 {
            // Single declaration: remove the whole statement
            let list_ext = self.arena.get_extended(list_idx)?;
            let stmt_idx = list_ext.parent;
            if stmt_idx.is_none() {
                return None;
            }
            let stmt_node = self.arena.get(stmt_idx)?;
            if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
                // Could be a for-loop initializer, etc. -- bail out.
                return None;
            }
            let (range, _) = self.declaration_removal_range(stmt_node);
            Some(range)
        } else {
            // Multiple declarations: remove just this one declaration + comma
            let var_decl_node = self.arena.get(var_decl_idx)?;
            let decl_start = var_decl_node.pos;
            let decl_end = var_decl_node.end;

            // Find our position in the list to handle comma removal
            let pos_in_list = list_data
                .declarations
                .nodes
                .iter()
                .position(|&idx| idx == var_decl_idx)?;

            let (remove_start, remove_end) = if pos_in_list < decl_count - 1 {
                // Not the last: remove from our start up to the next declaration's start
                let next_idx = list_data.declarations.nodes[pos_in_list + 1];
                let next_node = self.arena.get(next_idx)?;
                (decl_start, next_node.pos)
            } else {
                // Last in list: remove from previous declaration's end (after comma) to our end
                // We need to include the comma before us
                let prev_idx = list_data.declarations.nodes[pos_in_list - 1];
                let prev_node = self.arena.get(prev_idx)?;
                // The text between prev_node.end and our start contains ", "
                (prev_node.end, decl_end)
            };

            let start_pos = self.line_map.offset_to_position(remove_start, self.source);
            let end_pos = self.line_map.offset_to_position(remove_end, self.source);
            Some(Range::new(start_pos, end_pos))
        }
    }
}
