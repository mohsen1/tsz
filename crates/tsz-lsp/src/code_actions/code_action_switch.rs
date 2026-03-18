//! Add Missing Switch Cases code action.
//!
//! When the cursor is on a `switch` statement, generates case clauses
//! for all members of a union type or enum that aren't already covered.

use crate::rename::{TextEdit, WorkspaceEdit};
use crate::utils::find_node_at_offset;
use rustc_hash::FxHashMap;
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::syntax_kind_ext;

use super::code_action_provider::{CodeAction, CodeActionKind, CodeActionProvider};
use tsz_common::position::Range;

impl<'a> CodeActionProvider<'a> {
    /// Add missing case clauses to a switch statement.
    ///
    /// When the switch expression is a union type or enum, this generates
    /// case clauses for all values not already present.
    pub fn add_missing_switch_cases(&self, root: NodeIndex, range: Range) -> Option<CodeAction> {
        let start_offset = self.line_map.position_to_offset(range.start, self.source)?;
        let node_idx = find_node_at_offset(self.arena, start_offset);
        if node_idx.is_none() {
            return None;
        }

        // Walk up to find a switch statement
        let switch_idx = self.find_ancestor_of_kind(node_idx, syntax_kind_ext::SWITCH_STATEMENT)?;
        let switch_node = self.arena.get(switch_idx)?;

        // Get existing case values
        let existing_cases = self.collect_existing_case_values(switch_idx);

        // Get the switch expression text for generating enum member references
        let switch_data = self.arena.get_switch(switch_node)?;
        let _expr_text = self
            .source
            .get(
                self.arena.get(switch_data.expression)?.pos as usize
                    ..self.arena.get(switch_data.expression)?.end as usize,
            )?
            .trim();

        // Try to find enum members from the binder
        let enum_members = self.find_enum_members_for_expression(root, switch_data.expression);
        if enum_members.is_empty() {
            return None;
        }

        // Find which members are missing
        let missing: Vec<&str> = enum_members
            .iter()
            .map(|s| s.as_str())
            .filter(|member| !existing_cases.contains(&member.to_string()))
            .collect();

        if missing.is_empty() {
            return None;
        }

        // Find the insertion point (before the closing brace of the case block)
        let case_block = switch_data.case_block;
        let case_block_node = self.arena.get(case_block)?;
        let insert_offset = case_block_node.end.saturating_sub(1);
        let insert_pos = self.line_map.offset_to_position(insert_offset, self.source);

        // Get indentation
        let switch_pos = self
            .line_map
            .offset_to_position(switch_node.pos, self.source);
        let indent = self.get_indentation_at_position(&switch_pos);
        let case_indent = format!("{indent}    ");
        let body_indent = format!("{indent}        ");

        // Generate case clauses
        let mut new_text = String::new();
        for member in &missing {
            new_text.push_str(&format!(
                "\n{case_indent}case {member}:\n{body_indent}break;"
            ));
        }

        let mut changes = FxHashMap::default();
        changes.insert(
            self.file_name.clone(),
            vec![TextEdit {
                range: Range::new(insert_pos, insert_pos),
                new_text,
            }],
        );

        Some(CodeAction {
            title: format!("Add {} missing case clause(s)", missing.len()),
            kind: CodeActionKind::QuickFix,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
            data: None,
        })
    }

    /// Walk up the AST to find an ancestor of a specific kind.
    pub(super) fn find_ancestor_of_kind(&self, start: NodeIndex, kind: u16) -> Option<NodeIndex> {
        let mut current = start;
        for _ in 0..20 {
            let node = self.arena.get(current)?;
            if node.kind == kind {
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

    /// Collect the string values of existing case clauses in a switch statement.
    fn collect_existing_case_values(&self, switch_idx: NodeIndex) -> Vec<String> {
        let mut values = Vec::new();
        let switch_node = match self.arena.get(switch_idx) {
            Some(n) => n,
            None => return values,
        };
        let switch_data = match self.arena.get_switch(switch_node) {
            Some(d) => d,
            None => return values,
        };

        let case_block_node = match self.arena.get(switch_data.case_block) {
            Some(n) => n,
            None => return values,
        };

        // Scan children of the case block for case clauses
        for (i, node) in self.arena.nodes.iter().enumerate() {
            let idx = NodeIndex(i as u32);
            let parent = self
                .arena
                .get_extended(idx)
                .map_or(NodeIndex::NONE, |ext| ext.parent);

            if parent != switch_data.case_block {
                continue;
            }
            if node.pos > case_block_node.end {
                break;
            }
            if node.kind == syntax_kind_ext::CASE_CLAUSE {
                if let Some(case) = self.arena.get_case_clause(node) {
                    if let Some(text) = self.source.get(
                        self.arena.get(case.expression).map_or(0, |n| n.pos) as usize
                            ..self.arena.get(case.expression).map_or(0, |n| n.end) as usize,
                    ) {
                        values.push(text.trim().to_string());
                    }
                }
            }
        }

        values
    }

    /// Try to find enum members for a switch expression by resolving
    /// through the binder.
    fn find_enum_members_for_expression(
        &self,
        root: NodeIndex,
        expr_idx: NodeIndex,
    ) -> Vec<String> {
        // This is a simplified implementation that looks for direct enum references
        let mut members = Vec::new();

        // Check if the expression resolves to an enum-like symbol
        let mut walker = crate::resolver::ScopeWalker::new(self.arena, self.binder);
        let Some(symbol_id) = walker.resolve_node(root, expr_idx) else {
            return members;
        };

        let Some(symbol) = self.binder.symbols.get(symbol_id) else {
            return members;
        };

        // Check if any declaration is an enum
        for &decl_idx in &symbol.declarations {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            if decl_node.kind == syntax_kind_ext::ENUM_DECLARATION {
                if let Some(enum_data) = self.arena.get_enum(decl_node) {
                    let enum_name = self
                        .arena
                        .get_identifier_text(enum_data.name)
                        .unwrap_or("E");
                    for &member_idx in &enum_data.members.nodes {
                        if let Some(member_node) = self.arena.get(member_idx) {
                            if let Some(member) = self.arena.get_enum_member(member_node) {
                                if let Some(name) = self.arena.get_identifier_text(member.name) {
                                    members.push(format!("{enum_name}.{name}"));
                                }
                            }
                        }
                    }
                }
            }
        }

        members
    }
}
