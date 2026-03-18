//! Extract Type Alias refactoring for the LSP.
//!
//! Extracts a selected type annotation into a new `type` alias declaration
//! inserted before the enclosing statement, replacing the original type
//! with the alias name.

use crate::rename::{TextEdit, WorkspaceEdit};
use crate::utils::find_node_at_offset;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_parser::NodeIndex;
use tsz_parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

use super::code_action_provider::{CodeAction, CodeActionKind, CodeActionProvider};
use tsz_common::position::{Position, Range};

impl<'a> CodeActionProvider<'a> {
    /// Extract the selected type annotation to a new type alias.
    ///
    /// Example: Selecting `string | number` in `let x: string | number` produces:
    /// ```typescript
    /// type ExtractedType = string | number;
    /// let x: ExtractedType;
    /// ```
    pub fn extract_type_alias(&self, root: NodeIndex, range: Range) -> Option<CodeAction> {
        // 1. Convert range to offsets
        let start_offset = self.line_map.position_to_offset(range.start, self.source)?;
        let end_offset = self.line_map.position_to_offset(range.end, self.source)?;

        // 2. Find the type node that matches or contains this range
        let type_idx = self.find_type_node_at_range(root, start_offset, end_offset)?;

        // 3. Verify it's a type node
        let type_node = self.arena.get(type_idx)?;
        if !Self::is_extractable_type(type_node.kind) {
            return None;
        }

        // 4. Find the enclosing declaration to determine where to insert the type alias
        let decl_idx = self.find_enclosing_declaration(type_idx)?;
        let decl_node = self.arena.get(decl_idx)?;

        // 5. Generate a unique type alias name
        let type_name = self.unique_extracted_type_name(decl_idx);

        // 6. Extract the selected type text (snap to node boundaries)
        let node_start = type_node.pos;
        let node_end = type_node.end;
        let selected_text = self.source.get(node_start as usize..node_end as usize)?;

        let replacement_range = Range::new(
            self.line_map.offset_to_position(node_start, self.source),
            self.line_map.offset_to_position(node_end, self.source),
        );

        // 7. Create text edits:
        //    a) Insert type alias declaration before the enclosing declaration
        //    b) Replace the selected type with the alias name

        // Get the position to insert the type alias declaration
        let decl_pos = self.line_map.offset_to_position(decl_node.pos, self.source);
        let insert_pos = Position::new(decl_pos.line, 0);

        // Calculate indentation by looking at the declaration's line
        let indent = self.get_indentation_at_position(&decl_pos);

        let declaration = format!("{indent}type {type_name} = {selected_text};\n");

        let mut edits = Vec::new();

        // Insert the type alias declaration
        edits.push(TextEdit {
            range: Range {
                start: insert_pos,
                end: insert_pos,
            },
            new_text: declaration,
        });

        // Replace the type with the alias name
        edits.push(TextEdit {
            range: replacement_range,
            new_text: type_name.clone(),
        });

        // Create the workspace edit
        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), edits);

        Some(CodeAction {
            title: format!("Extract to type alias '{type_name}'"),
            kind: CodeActionKind::RefactorExtract,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
            data: None,
        })
    }

    /// Check if a syntax kind represents a type node that can be extracted.
    const fn is_extractable_type(kind: u16) -> bool {
        matches!(
            kind,
            syntax_kind_ext::TYPE_REFERENCE
                | syntax_kind_ext::UNION_TYPE
                | syntax_kind_ext::INTERSECTION_TYPE
                | syntax_kind_ext::TUPLE_TYPE
                | syntax_kind_ext::ARRAY_TYPE
                | syntax_kind_ext::FUNCTION_TYPE
                | syntax_kind_ext::CONSTRUCTOR_TYPE
                | syntax_kind_ext::TYPE_LITERAL
                | syntax_kind_ext::MAPPED_TYPE
                | syntax_kind_ext::CONDITIONAL_TYPE
                | syntax_kind_ext::INDEXED_ACCESS_TYPE
                | syntax_kind_ext::PARENTHESIZED_TYPE
                | syntax_kind_ext::TYPE_QUERY
                | syntax_kind_ext::TYPE_OPERATOR
                | syntax_kind_ext::TEMPLATE_LITERAL_TYPE
                | syntax_kind_ext::IMPORT_TYPE
                | syntax_kind_ext::LITERAL_TYPE
                | syntax_kind_ext::THIS_TYPE
                | syntax_kind_ext::REST_TYPE
                | syntax_kind_ext::OPTIONAL_TYPE
                | syntax_kind_ext::INFER_TYPE
                | syntax_kind_ext::TYPE_PREDICATE
                | syntax_kind_ext::NAMED_TUPLE_MEMBER
        )
    }

    /// Check if a syntax kind is any type node (including keyword types).
    const fn is_type_node(kind: u16) -> bool {
        Self::is_extractable_type(kind)
            || kind == SyntaxKind::StringKeyword as u16
            || kind == SyntaxKind::NumberKeyword as u16
            || kind == SyntaxKind::BooleanKeyword as u16
            || kind == SyntaxKind::AnyKeyword as u16
            || kind == SyntaxKind::VoidKeyword as u16
            || kind == SyntaxKind::NeverKeyword as u16
            || kind == SyntaxKind::UndefinedKeyword as u16
            || kind == SyntaxKind::NullKeyword as u16
            || kind == SyntaxKind::ObjectKeyword as u16
            || kind == SyntaxKind::SymbolKeyword as u16
            || kind == SyntaxKind::BigIntKeyword as u16
            || kind == SyntaxKind::UnknownKeyword as u16
    }

    /// Find a type node that matches or contains the given range.
    fn find_type_node_at_range(&self, _root: NodeIndex, start: u32, end: u32) -> Option<NodeIndex> {
        let mut current = find_node_at_offset(self.arena, start);
        while current.is_some() {
            let node = self.arena.get(current)?;
            if node.pos <= start && node.end >= end && Self::is_type_node(node.kind) {
                return Some(current);
            }
            let ext = self.arena.get_extended(current)?;
            current = ext.parent;
        }
        None
    }

    /// Find the enclosing declaration for a given type node.
    /// This walks up to find the nearest statement or declaration that can precede
    /// a type alias insertion.
    fn find_enclosing_declaration(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = node_idx;
        while current.is_some() {
            let node = self.arena.get(current)?;
            if self.is_type_alias_insertion_target(node.kind) {
                return Some(current);
            }
            let ext = self.arena.get_extended(current)?;
            current = ext.parent;
        }
        None
    }

    /// Check if a syntax kind is a valid target before which we can insert a type alias.
    const fn is_type_alias_insertion_target(&self, kind: u16) -> bool {
        matches!(
            kind,
            syntax_kind_ext::VARIABLE_STATEMENT
                | syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::INTERFACE_DECLARATION
                | syntax_kind_ext::TYPE_ALIAS_DECLARATION
                | syntax_kind_ext::EXPRESSION_STATEMENT
                | syntax_kind_ext::RETURN_STATEMENT
                | syntax_kind_ext::IF_STATEMENT
                | syntax_kind_ext::FOR_STATEMENT
                | syntax_kind_ext::FOR_IN_STATEMENT
                | syntax_kind_ext::FOR_OF_STATEMENT
                | syntax_kind_ext::WHILE_STATEMENT
                | syntax_kind_ext::DO_STATEMENT
                | syntax_kind_ext::EXPORT_DECLARATION
                | syntax_kind_ext::ENUM_DECLARATION
        )
    }

    /// Generate a unique type alias name scoped to the insertion point.
    fn unique_extracted_type_name(&self, stmt_idx: NodeIndex) -> String {
        let mut names = FxHashSet::default();
        if let Some(scope_id) = self.find_enclosing_scope_id(stmt_idx) {
            self.collect_scope_names(scope_id, &mut names);
        }

        let base = "ExtractedType";
        if !names.contains(base) {
            return base.to_string();
        }

        let mut suffix = 2;
        loop {
            let candidate = format!("{base}{suffix}");
            if !names.contains(&candidate) {
                return candidate;
            }
            suffix += 1;
        }
    }
}
