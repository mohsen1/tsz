//! Convert between named and default exports.
//!
//! - `export function foo()` → `export default function foo()` and vice versa
//! - `export class Foo` → `export default class Foo` and vice versa

use crate::rename::{TextEdit, WorkspaceEdit};
use crate::utils::find_node_at_offset;
use rustc_hash::FxHashMap;
use tsz_parser::NodeIndex;
use tsz_parser::syntax_kind_ext;

use super::code_action_provider::{CodeAction, CodeActionKind, CodeActionProvider};
use tsz_common::position::Range;

impl<'a> CodeActionProvider<'a> {
    /// Convert a named export to a default export.
    /// `export function foo()` → `export default function foo()`
    pub fn convert_to_default_export(&self, _root: NodeIndex, range: Range) -> Option<CodeAction> {
        let start_offset = self.line_map.position_to_offset(range.start, self.source)?;
        let decl_idx = self.find_exported_declaration(start_offset)?;
        let decl_node = self.arena.get(decl_idx)?;

        // Check it has an export modifier but not default
        let decl_text = self
            .source
            .get(decl_node.pos as usize..decl_node.end as usize)?;

        if !decl_text.starts_with("export ") || decl_text.starts_with("export default ") {
            return None;
        }

        // Replace "export " with "export default "
        let new_text = decl_text.replacen("export ", "export default ", 1);

        let replace_start = self.line_map.offset_to_position(decl_node.pos, self.source);
        let replace_end = self.line_map.offset_to_position(decl_node.end, self.source);

        let edit = TextEdit {
            range: Range::new(replace_start, replace_end),
            new_text,
        };

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), vec![edit]);

        Some(CodeAction {
            title: "Convert to default export".to_string(),
            kind: CodeActionKind::Refactor,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
            data: None,
        })
    }

    /// Convert a default export to a named export.
    /// `export default function foo()` → `export function foo()`
    pub fn convert_to_named_export(&self, _root: NodeIndex, range: Range) -> Option<CodeAction> {
        let start_offset = self.line_map.position_to_offset(range.start, self.source)?;
        let decl_idx = self.find_default_exported_declaration(start_offset)?;
        let decl_node = self.arena.get(decl_idx)?;

        let decl_text = self
            .source
            .get(decl_node.pos as usize..decl_node.end as usize)?;

        if !decl_text.starts_with("export default ") {
            return None;
        }

        let new_text = decl_text.replacen("export default ", "export ", 1);

        let replace_start = self.line_map.offset_to_position(decl_node.pos, self.source);
        let replace_end = self.line_map.offset_to_position(decl_node.end, self.source);

        let edit = TextEdit {
            range: Range::new(replace_start, replace_end),
            new_text,
        };

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), vec![edit]);

        Some(CodeAction {
            title: "Convert to named export".to_string(),
            kind: CodeActionKind::Refactor,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
            data: None,
        })
    }

    fn find_exported_declaration(&self, offset: u32) -> Option<NodeIndex> {
        let mut current = find_node_at_offset(self.arena, offset);
        while current.is_some() {
            let node = self.arena.get(current)?;
            match node.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::INTERFACE_DECLARATION
                    || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    || k == syntax_kind_ext::ENUM_DECLARATION
                    || k == syntax_kind_ext::VARIABLE_STATEMENT =>
                {
                    // Check if it has export modifier
                    let text = self.source.get(node.pos as usize..node.end as usize)?;
                    if text.starts_with("export ") && !text.starts_with("export default ") {
                        return Some(current);
                    }
                }
                _ => {}
            }
            current = self.arena.get_extended(current)?.parent;
        }
        None
    }

    fn find_default_exported_declaration(&self, offset: u32) -> Option<NodeIndex> {
        let mut current = find_node_at_offset(self.arena, offset);
        while current.is_some() {
            let node = self.arena.get(current)?;
            let text = self
                .source
                .get(node.pos as usize..node.end as usize)
                .unwrap_or("");
            if text.starts_with("export default ") {
                return Some(current);
            }
            current = self.arena.get_extended(current)?.parent;
        }
        None
    }
}
