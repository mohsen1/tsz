//! Convert Namespace Import code action.
//!
//! Converts between namespace imports and named imports:
//! - `import * as ns from "mod"` → `import { a, b, c } from "mod"`
//! - `import { a, b, c } from "mod"` → `import * as ns from "mod"`

use crate::rename::{TextEdit, WorkspaceEdit};
use crate::resolver::ScopeWalker;
use crate::utils::find_node_at_offset;
use rustc_hash::FxHashMap;
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::syntax_kind_ext;

use super::code_action_provider::{CodeAction, CodeActionKind, CodeActionProvider};
use tsz_common::position::Range;

impl<'a> CodeActionProvider<'a> {
    /// Convert a namespace import to named imports.
    ///
    /// `import * as ns from "mod"` → `import { used1, used2 } from "mod"`
    ///
    /// Scans all usages of `ns.X` in the file and collects the member names.
    pub fn convert_namespace_to_named(&self, root: NodeIndex, range: Range) -> Option<CodeAction> {
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

        // Check if it's a namespace import (import * as ns from "mod")
        let clause_idx = import_data.import_clause;
        let clause_node = self.arena.get(clause_idx)?;
        let clause = self.arena.get_import_clause(clause_node)?;

        // The namespace import is in clause.named_bindings
        let ns_idx = clause.named_bindings;
        let ns_node = self.arena.get(ns_idx)?;
        if ns_node.kind != syntax_kind_ext::NAMESPACE_IMPORT {
            return None;
        }

        // Get the namespace name
        let ns_name_text = self
            .source
            .get(ns_node.pos as usize..ns_node.end as usize)?;
        let _ns_name = ns_name_text
            .strip_prefix("* as ")
            .unwrap_or(ns_name_text)
            .trim();

        // Get the module specifier text
        let module_spec = self
            .source
            .get(
                self.arena.get(import_data.module_specifier)?.pos as usize
                    ..self.arena.get(import_data.module_specifier)?.end as usize,
            )?
            .trim();

        // Find all usages of `ns.member` and collect member names
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let symbol_id = walker.resolve_node(root, ns_idx)?;

        let ref_nodes = walker.find_references(root, symbol_id);
        let mut member_names: Vec<String> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for &ref_idx in &ref_nodes {
            // Check if the reference is used in a property access: ns.member
            if let Some(ext) = self.arena.get_extended(ref_idx) {
                let parent = ext.parent;
                if let Some(parent_node) = self.arena.get(parent)
                    && parent_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    && let Some(access) = self.arena.get_access_expr(parent_node)
                    && let Some(name) = self.arena.get_identifier_text(access.name_or_argument)
                    && seen.insert(name.to_string())
                {
                    member_names.push(name.to_string());
                }
            }
        }

        if member_names.is_empty() {
            return None;
        }

        member_names.sort();

        // Build the replacement import
        let new_import = format!(
            "import {{ {} }} from {};",
            member_names.join(", "),
            module_spec
        );

        let import_start = self
            .line_map
            .offset_to_position(import_node.pos, self.source);
        let import_end = self
            .line_map
            .offset_to_position(import_node.end, self.source);

        let mut edits = vec![TextEdit {
            range: Range::new(import_start, import_end),
            new_text: new_import,
        }];

        // Also replace all `ns.member` usages with just `member`
        for &ref_idx in &ref_nodes {
            if let Some(ext) = self.arena.get_extended(ref_idx) {
                let parent = ext.parent;
                if let Some(parent_node) = self.arena.get(parent)
                    && parent_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    && let Some(access) = self.arena.get_access_expr(parent_node)
                    && let Some(name) = self.arena.get_identifier_text(access.name_or_argument)
                {
                    let access_start = self
                        .line_map
                        .offset_to_position(parent_node.pos, self.source);
                    let access_end = self
                        .line_map
                        .offset_to_position(parent_node.end, self.source);
                    edits.push(TextEdit {
                        range: Range::new(access_start, access_end),
                        new_text: name.to_string(),
                    });
                }
            }
        }

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), edits);

        Some(CodeAction {
            title: "Convert namespace import to named imports".to_string(),
            kind: CodeActionKind::Refactor,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
            data: None,
        })
    }

    /// Convert named imports to a namespace import.
    ///
    /// `import { a, b, c } from "mod"` → `import * as mod from "mod"`
    pub fn convert_named_to_namespace(&self, _root: NodeIndex, range: Range) -> Option<CodeAction> {
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

        // Check if it has named bindings (import { a, b } from "mod")
        let clause_idx = import_data.import_clause;
        let clause_node = self.arena.get(clause_idx)?;
        let clause = self.arena.get_import_clause(clause_node)?;

        let named_idx = clause.named_bindings;
        let named_node = self.arena.get(named_idx)?;
        if named_node.kind != syntax_kind_ext::NAMED_IMPORTS {
            return None;
        }

        // Get the module specifier to derive namespace name
        let module_spec_text = self
            .source
            .get(
                self.arena.get(import_data.module_specifier)?.pos as usize
                    ..self.arena.get(import_data.module_specifier)?.end as usize,
            )?
            .trim();

        // Derive namespace name from module specifier
        let ns_name = derive_namespace_name(module_spec_text);

        let module_spec = module_spec_text;

        let new_import = format!("import * as {ns_name} from {module_spec};");

        let import_start = self
            .line_map
            .offset_to_position(import_node.pos, self.source);
        let import_end = self
            .line_map
            .offset_to_position(import_node.end, self.source);

        let mut changes = FxHashMap::default();
        changes.insert(
            self.file_name.clone(),
            vec![TextEdit {
                range: Range::new(import_start, import_end),
                new_text: new_import,
            }],
        );

        Some(CodeAction {
            title: format!("Convert to namespace import (* as {ns_name})"),
            kind: CodeActionKind::Refactor,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
            data: None,
        })
    }
}

/// Derive a namespace name from a module specifier.
/// E.g., `"./utils"` → `utils`, `"lodash"` → `lodash`, `"@scope/pkg"` → `pkg`
fn derive_namespace_name(specifier: &str) -> String {
    let unquoted = specifier.trim_matches(|c| c == '"' || c == '\'');
    let last_segment = unquoted.rsplit('/').next().unwrap_or(unquoted);
    // Remove file extension
    let name = if let Some(dot) = last_segment.rfind('.') {
        &last_segment[..dot]
    } else {
        last_segment
    };
    // Convert to valid identifier (replace hyphens with underscores)
    name.replace('-', "_")
}
