//! Sort Imports code action (separate from Organize Imports).
//!
//! While Organize Imports sorts import declarations by module specifier,
//! this action sorts the *specifiers within* a single import statement:
//! `import { z, a, m } from "mod"` → `import { a, m, z } from "mod"`

use crate::rename::{TextEdit, WorkspaceEdit};
use crate::utils::find_node_at_offset;
use rustc_hash::FxHashMap;
use tsz_parser::NodeIndex;
use tsz_parser::syntax_kind_ext;

use super::code_action_provider::{CodeAction, CodeActionKind, CodeActionProvider};
use tsz_common::position::Range;

impl<'a> CodeActionProvider<'a> {
    /// Sort import specifiers within a single import statement.
    ///
    /// Given `import { z, a, m } from "mod"`, produces
    /// `import { a, m, z } from "mod"`.
    pub fn sort_import_specifiers(&self, _root: NodeIndex, range: Range) -> Option<CodeAction> {
        let start_offset = self.line_map.position_to_offset(range.start, self.source)?;
        let node_idx = find_node_at_offset(self.arena, start_offset);
        if node_idx.is_none() {
            return None;
        }

        // Find the import declaration
        let import_idx =
            self.find_ancestor_of_kind(node_idx, syntax_kind_ext::IMPORT_DECLARATION)?;
        let import_node = self.arena.get(import_idx)?;
        let import_data = self.arena.get_import_decl(import_node)?;

        // Get the named imports
        let clause_idx = import_data.import_clause;
        let clause_node = self.arena.get(clause_idx)?;
        let clause = self.arena.get_import_clause(clause_node)?;

        let named_idx = clause.named_bindings;
        let named_node = self.arena.get(named_idx)?;
        if named_node.kind != syntax_kind_ext::NAMED_IMPORTS {
            return None;
        }

        let named = self.arena.get_named_imports(named_node)?;
        if named.elements.nodes.len() <= 1 {
            return None; // Nothing to sort
        }

        // Collect specifier texts
        let mut specifier_texts: Vec<String> = Vec::new();
        for &spec_idx in &named.elements.nodes {
            let spec_node = self.arena.get(spec_idx)?;
            let text = self
                .source
                .get(spec_node.pos as usize..spec_node.end as usize)?
                .trim()
                .to_string();
            specifier_texts.push(text);
        }

        // Sort them
        let mut sorted = specifier_texts.clone();
        sorted.sort_by(|a, b| {
            let a_lower = a.to_lowercase();
            let b_lower = b.to_lowercase();
            a_lower.cmp(&b_lower)
        });

        // Check if already sorted
        if specifier_texts == sorted {
            return None;
        }

        // Build the replacement text
        let new_named = format!("{{ {} }}", sorted.join(", "));

        let named_start = self
            .line_map
            .offset_to_position(named_node.pos, self.source);
        let named_end = self
            .line_map
            .offset_to_position(named_node.end, self.source);

        let mut changes = FxHashMap::default();
        changes.insert(
            self.file_name.clone(),
            vec![TextEdit {
                range: Range::new(named_start, named_end),
                new_text: new_named,
            }],
        );

        Some(CodeAction {
            title: "Sort import specifiers".to_string(),
            kind: CodeActionKind::SourceOrganizeImports,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
            data: None,
        })
    }
}
