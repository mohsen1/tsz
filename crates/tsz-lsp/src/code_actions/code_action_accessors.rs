//! Generate Getter/Setter code actions for class properties.
//!
//! When the cursor is on a class property declaration, this module provides
//! up to three refactoring actions:
//! 1. Generate getter
//! 2. Generate setter
//! 3. Generate getter and setter
//!
//! The generated accessors rename the original property to a private backing
//! field prefixed with `_`.

use super::code_action_provider::{CodeAction, CodeActionKind, CodeActionProvider};
use crate::rename::{TextEdit, WorkspaceEdit};
use crate::utils::find_node_at_offset;
use rustc_hash::FxHashMap;
use tsz_common::position::{Position, Range};
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::syntax_kind_ext;

impl<'a> CodeActionProvider<'a> {
    /// Generate getter and/or setter code actions for the class property at the
    /// given cursor range.
    ///
    /// Returns up to three `CodeAction` entries when the cursor sits on a
    /// `PropertyDeclaration` inside a class body.
    pub fn generate_accessors(&self, root: NodeIndex, range: Range) -> Vec<CodeAction> {
        let mut actions = Vec::new();

        let Some(info) = self.find_property_declaration_info(root, range) else {
            return actions;
        };

        let type_suffix = info
            .type_text
            .as_ref()
            .map_or(String::new(), |t| format!(": {t}"));

        let backing_field = format!("_{}", info.name);
        let indent = &info.indent;

        // -- Generate getter -------------------------------------------------
        let getter_text = format!(
            "\n{indent}get {name}(){type_suffix} {{\n\
             {indent}{unit}return this.{backing};\n\
             {indent}}}",
            name = info.name,
            type_suffix = type_suffix,
            backing = backing_field,
            unit = info.indent_unit,
        );

        // -- Generate setter -------------------------------------------------
        let param_type = info
            .type_text
            .as_ref()
            .map_or(String::new(), |t| format!(": {t}"));
        let setter_text = format!(
            "\n{indent}set {name}(value{param_type}) {{\n\
             {indent}{unit}this.{backing} = value;\n\
             {indent}}}",
            name = info.name,
            param_type = param_type,
            backing = backing_field,
            unit = info.indent_unit,
        );

        // Shared helper: build a rename edit for the backing field plus an
        // insertion edit for the accessor body.
        let rename_edit = TextEdit {
            range: Range::new(info.name_start, info.name_end),
            new_text: backing_field,
        };

        let insert_pos = info.insert_position;

        // 1. Generate getter
        {
            let insert_edit = TextEdit {
                range: Range::new(insert_pos, insert_pos),
                new_text: getter_text.clone(),
            };
            let mut changes = FxHashMap::default();
            changes.insert(
                self.file_name.clone(),
                vec![rename_edit.clone(), insert_edit],
            );
            actions.push(CodeAction {
                title: "Generate getter".to_string(),
                kind: CodeActionKind::Refactor,
                edit: Some(WorkspaceEdit { changes }),
                is_preferred: false,
                data: None,
            });
        }

        // 2. Generate setter
        {
            let insert_edit = TextEdit {
                range: Range::new(insert_pos, insert_pos),
                new_text: setter_text.clone(),
            };
            let mut changes = FxHashMap::default();
            changes.insert(
                self.file_name.clone(),
                vec![rename_edit.clone(), insert_edit],
            );
            actions.push(CodeAction {
                title: "Generate setter".to_string(),
                kind: CodeActionKind::Refactor,
                edit: Some(WorkspaceEdit { changes }),
                is_preferred: false,
                data: None,
            });
        }

        // 3. Generate getter and setter
        {
            let both_text = format!("{getter_text}{setter_text}");
            let insert_edit = TextEdit {
                range: Range::new(insert_pos, insert_pos),
                new_text: both_text,
            };
            let mut changes = FxHashMap::default();
            changes.insert(self.file_name.clone(), vec![rename_edit, insert_edit]);
            actions.push(CodeAction {
                title: "Generate getter and setter".to_string(),
                kind: CodeActionKind::Refactor,
                edit: Some(WorkspaceEdit { changes }),
                is_preferred: false,
                data: None,
            });
        }

        actions
    }

    // -------------------------------------------------------------------------
    // Helpers
    // -------------------------------------------------------------------------

    /// Locate the `PROPERTY_DECLARATION` node at the cursor and extract the
    /// information needed to build accessor code actions.
    fn find_property_declaration_info(
        &self,
        _root: NodeIndex,
        range: Range,
    ) -> Option<PropertyDeclInfo> {
        let offset = self.line_map.position_to_offset(range.start, self.source)?;
        let node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_none() {
            return None;
        }

        // Walk up to find the PROPERTY_DECLARATION node.
        let prop_idx = self.walk_to_property_declaration(node_idx)?;
        let prop_node = self.arena.get(prop_idx)?;
        let prop_data = self.arena.get_property_decl(prop_node)?;

        // Property name
        let name = self.arena.get_identifier_text(prop_data.name)?.to_string();

        // Name positions (for renaming to _name)
        let name_node = self.arena.get(prop_data.name)?;
        let name_start = self.line_map.offset_to_position(name_node.pos, self.source);
        let name_end = self.line_map.offset_to_position(name_node.end, self.source);

        // Type annotation text (if present)
        let type_text = if prop_data.type_annotation.is_some() {
            let type_node = self.arena.get(prop_data.type_annotation)?;
            let text = self
                .source
                .get(type_node.pos as usize..type_node.end as usize)?
                .trim()
                .to_string();
            Some(text)
        } else {
            None
        };

        // Indentation of the property line
        let indent = self.indent_at_offset(prop_node.pos);
        let indent_unit = self.indent_unit_from(&indent).to_string();

        // Insert position: end of the property declaration line
        // Find the end of the line (including semicolons/trailing content)
        let mut end_offset = prop_node.end as usize;
        if let Some(rest) = self.source.get(end_offset..) {
            for &byte in rest.as_bytes() {
                if byte == b'\n' {
                    break;
                }
                if byte == b'\r' {
                    break;
                }
                end_offset += 1;
            }
        }
        let insert_position = self
            .line_map
            .offset_to_position(end_offset as u32, self.source);

        Some(PropertyDeclInfo {
            name,
            name_start,
            name_end,
            type_text,
            indent,
            indent_unit,
            insert_position,
        })
    }

    /// Walk from the given node upward to find a `PROPERTY_DECLARATION`.
    fn walk_to_property_declaration(&self, start: NodeIndex) -> Option<NodeIndex> {
        let mut current = start;
        for _ in 0..10 {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
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
}

/// Intermediate data extracted from a property declaration, used to build the
/// code action edits.
struct PropertyDeclInfo {
    /// The property name (e.g. `foo`).
    name: String,
    /// Start position of the property name token.
    name_start: Position,
    /// End position of the property name token.
    name_end: Position,
    /// Type annotation text if present (e.g. `string`).
    type_text: Option<String>,
    /// Leading whitespace of the property line.
    indent: String,
    /// One indentation unit (tab or two spaces).
    indent_unit: String,
    /// Position where accessor bodies should be inserted (end of property line).
    insert_position: Position,
}
