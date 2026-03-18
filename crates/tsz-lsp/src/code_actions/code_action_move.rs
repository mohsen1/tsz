//! Move to file refactoring.
//!
//! Move a top-level declaration (function, class, interface, type, const) to a different file.
//! Automatically adds an import in the source file pointing to the new location.
//!
//! Note: This provides the code action metadata. The actual file creation and
//! cross-project import updates require workspace-level coordination that happens
//! at the LSP server layer.

use crate::rename::{TextEdit, WorkspaceEdit};
use rustc_hash::FxHashMap;
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::syntax_kind_ext;

use super::code_action_provider::{CodeAction, CodeActionKind, CodeActionProvider};
use tsz_common::position::Range;

impl<'a> CodeActionProvider<'a> {
    /// Offer to move a top-level declaration to a new file.
    ///
    /// When cursor is on a top-level function, class, interface, type alias, enum,
    /// or const declaration, offer "Move to a new file".
    pub fn move_to_new_file(&self, root: NodeIndex, range: Range) -> Option<CodeAction> {
        let start_offset = self.line_map.position_to_offset(range.start, self.source)?;
        let decl_idx = self.find_top_level_declaration_at_offset(root, start_offset)?;
        let decl_node = self.arena.get(decl_idx)?;

        // Get the declaration name
        let decl_name = self.get_declaration_name(decl_idx)?;

        // Get the declaration text
        let decl_text = self
            .source
            .get(decl_node.pos as usize..decl_node.end as usize)?;

        // Compute the new file name from the declaration name
        let new_file_name = to_kebab_case(&decl_name);

        // Build the edit: remove declaration from current file and add import
        let decl_start = self.line_map.offset_to_position(decl_node.pos, self.source);
        let mut decl_end_offset = decl_node.end;

        // Include trailing newline
        if let Some(rest) = self.source.get(decl_end_offset as usize..) {
            for &b in rest.as_bytes() {
                if b == b'\n' {
                    decl_end_offset += 1;
                    break;
                }
                if b == b'\r' {
                    decl_end_offset += 1;
                    if rest
                        .as_bytes()
                        .get((decl_end_offset - decl_node.end) as usize)
                        == Some(&b'\n')
                    {
                        decl_end_offset += 1;
                    }
                    break;
                }
                if !b.is_ascii_whitespace() {
                    break;
                }
                decl_end_offset += 1;
            }
        }

        let decl_end = self
            .line_map
            .offset_to_position(decl_end_offset, self.source);

        let remove_edit = TextEdit {
            range: Range::new(decl_start, decl_end),
            new_text: String::new(),
        };

        // Add import statement at the top of the file
        let import_text = format!("import {{ {decl_name} }} from \"./{new_file_name}\";\n");

        let import_insert_offset = self.find_import_insert_position(root);
        let import_pos = self
            .line_map
            .offset_to_position(import_insert_offset, self.source);

        let import_edit = TextEdit {
            range: Range::new(import_pos, import_pos),
            new_text: import_text,
        };

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), vec![import_edit, remove_edit]);

        // Also create the new file with the declaration
        let extension = self.file_name.rsplit('.').next().unwrap_or("ts");
        let new_file_path = if let Some(dir) = self.file_name.rsplit_once('/') {
            format!("{}/{new_file_name}.{extension}", dir.0)
        } else {
            format!("{new_file_name}.{extension}")
        };

        let mut export_text = String::new();
        if !decl_text.contains("export ") {
            export_text.push_str("export ");
        }
        export_text.push_str(decl_text);
        if !export_text.ends_with('\n') {
            export_text.push('\n');
        }

        changes.insert(
            new_file_path,
            vec![TextEdit {
                range: Range::new(
                    tsz_common::position::Position::new(0, 0),
                    tsz_common::position::Position::new(0, 0),
                ),
                new_text: export_text,
            }],
        );

        Some(CodeAction {
            title: format!("Move '{decl_name}' to a new file"),
            kind: CodeActionKind::Refactor,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
            data: None,
        })
    }

    /// Find a top-level declaration at the given offset.
    fn find_top_level_declaration_at_offset(
        &self,
        root: NodeIndex,
        offset: u32,
    ) -> Option<NodeIndex> {
        let source_node = self.arena.get(root)?;
        let source_data = self.arena.get_source_file(source_node)?;

        for &stmt_idx in &source_data.statements.nodes {
            let stmt_node = self.arena.get(stmt_idx)?;
            if offset < stmt_node.pos || offset > stmt_node.end {
                continue;
            }

            // Check if it's a declaration we can move
            match stmt_node.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::INTERFACE_DECLARATION
                    || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    || k == syntax_kind_ext::ENUM_DECLARATION =>
                {
                    return Some(stmt_idx);
                }
                k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                    return Some(stmt_idx);
                }
                _ => {}
            }
        }

        None
    }

    /// Get the name of a declaration.
    fn get_declaration_name(&self, idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(idx)?;
        match node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                let func = self.arena.get_function(node)?;
                self.arena.get_identifier_text(func.name).map(String::from)
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                let class = self.arena.get_class(node)?;
                self.arena.get_identifier_text(class.name).map(String::from)
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                let iface = self.arena.get_interface(node)?;
                self.arena.get_identifier_text(iface.name).map(String::from)
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                let alias = self.arena.get_type_alias(node)?;
                self.arena.get_identifier_text(alias.name).map(String::from)
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                let enum_decl = self.arena.get_enum(node)?;
                self.arena
                    .get_identifier_text(enum_decl.name)
                    .map(String::from)
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                // Get name from the first variable declaration
                self.get_first_variable_name(idx)
            }
            _ => None,
        }
    }

    fn get_first_variable_name(&self, stmt_idx: NodeIndex) -> Option<String> {
        // Walk children to find variable declaration list, then first declaration
        let children = self.arena.get_children(stmt_idx);
        for child_idx in children {
            let child = self.arena.get(child_idx)?;
            if child.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                let list_children = self.arena.get_children(child_idx);
                for decl_idx in list_children {
                    let decl_node = self.arena.get(decl_idx)?;
                    if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                        let decl = self.arena.get_variable_declaration(decl_node)?;
                        return self.arena.get_identifier_text(decl.name).map(String::from);
                    }
                }
            }
        }
        None
    }

    /// Find the position after the last import statement for inserting new imports.
    fn find_import_insert_position(&self, root: NodeIndex) -> u32 {
        let Some(source_node) = self.arena.get(root) else {
            return 0;
        };
        let Some(source_data) = self.arena.get_source_file(source_node) else {
            return 0;
        };

        let mut last_import_end = 0u32;
        for &stmt_idx in &source_data.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind == syntax_kind_ext::IMPORT_DECLARATION {
                last_import_end = stmt_node.end;
                // Skip trailing whitespace/newline
                if let Some(rest) = self.source.get(last_import_end as usize..) {
                    for &b in rest.as_bytes() {
                        if b == b'\n' {
                            last_import_end += 1;
                            break;
                        }
                        if b == b'\r' {
                            last_import_end += 1;
                            if rest
                                .as_bytes()
                                .get((last_import_end - stmt_node.end) as usize)
                                == Some(&b'\n')
                            {
                                last_import_end += 1;
                            }
                            break;
                        }
                        if !b.is_ascii_whitespace() {
                            break;
                        }
                        last_import_end += 1;
                    }
                }
            }
        }

        last_import_end
    }
}

/// Convert a PascalCase or camelCase name to kebab-case.
fn to_kebab_case(name: &str) -> String {
    let mut result = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push('-');
            }
            result.push(ch.to_lowercase().next().unwrap_or(ch));
        } else {
            result.push(ch);
        }
    }
    result
}
