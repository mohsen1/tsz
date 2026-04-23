//! Extract interface from class.
//!
//! Given a class, extract its public API into an interface and add
//! `implements InterfaceName` to the class.

use crate::rename::{TextEdit, WorkspaceEdit};
use crate::utils::find_node_at_offset;
use rustc_hash::FxHashMap;
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

use super::code_action_provider::{CodeAction, CodeActionKind, CodeActionProvider};
use tsz_common::position::Range;

impl<'a> CodeActionProvider<'a> {
    /// Extract an interface from a class's public members.
    pub fn extract_interface_from_class(
        &self,
        _root: NodeIndex,
        range: Range,
    ) -> Option<CodeAction> {
        let start_offset = self.line_map.position_to_offset(range.start, self.source)?;

        // Find class at cursor
        let class_idx = self.find_class_at_offset(start_offset)?;
        let class_node = self.arena.get(class_idx)?;
        let class_data = self.arena.get_class(class_node)?;

        // Get class name
        let class_name = self.arena.get_identifier_text(class_data.name)?;
        let interface_name = format!("I{class_name}");

        // Collect public members for the interface
        let mut interface_members = Vec::new();
        let class_indent = self.indent_at_offset(class_node.pos);
        let member_indent = {
            let unit = self.indent_unit_from(&class_indent);
            format!("{class_indent}{unit}")
        };

        for &member_idx in &class_data.members.nodes {
            let member_node = self.arena.get(member_idx)?;

            // Skip private/protected members, static members, constructors
            if self.is_private_or_protected(member_node) {
                continue;
            }
            if self.is_static_member(member_node) {
                continue;
            }

            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let prop = self.arena.get_property_decl(member_node)?;
                    let name = self.arena.get_identifier_text(prop.name)?;
                    let type_text = if prop.type_annotation.is_some() {
                        let type_node = self.arena.get(prop.type_annotation)?;
                        self.source
                            .get(type_node.pos as usize..type_node.end as usize)
                            .unwrap_or("any")
                    } else {
                        "any"
                    };
                    let optional = if prop.question_token { "?" } else { "" };
                    interface_members
                        .push(format!("{member_indent}{name}{optional}: {type_text};"));
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let func = self.arena.get_function(member_node)?;
                    let name = self.arena.get_identifier_text(func.name)?;

                    let type_params = func
                        .type_parameters
                        .as_ref()
                        .and_then(|tp| self.source.get(tp.pos as usize..tp.end as usize))
                        .unwrap_or("");

                    let params_text = self
                        .source
                        .get(func.parameters.pos as usize..func.parameters.end as usize)
                        .unwrap_or("");

                    let return_type = if func.type_annotation.is_some() {
                        let type_node = self.arena.get(func.type_annotation)?;
                        let rt = self
                            .source
                            .get(type_node.pos as usize..type_node.end as usize)
                            .unwrap_or("void");
                        format!(": {rt}")
                    } else {
                        String::new()
                    };

                    interface_members.push(format!(
                        "{member_indent}{name}{type_params}({params_text}){return_type};"
                    ));
                }
                _ => {}
            }
        }

        if interface_members.is_empty() {
            return None;
        }

        // Build the interface text
        let members_text = interface_members.join("\n");
        let interface_text =
            format!("interface {interface_name} {{\n{members_text}\n{class_indent}}}\n\n");

        // Insert interface before the class
        let class_start = self
            .line_map
            .offset_to_position(class_node.pos, self.source);
        let insert_edit = TextEdit {
            range: Range::new(class_start, class_start),
            new_text: interface_text,
        };

        // Add `implements InterfaceName` to the class
        let mut edits = vec![insert_edit];

        // Find position to insert `implements` clause
        if let Some(implements_edit) =
            self.build_implements_edit(class_idx, class_data, &interface_name)
        {
            edits.push(implements_edit);
        }

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), edits);

        Some(CodeAction {
            title: format!("Extract interface '{interface_name}'"),
            kind: CodeActionKind::RefactorExtract,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
            data: None,
        })
    }

    fn find_class_at_offset(&self, offset: u32) -> Option<NodeIndex> {
        let mut current = find_node_at_offset(self.arena, offset);
        while current.is_some() {
            let node = self.arena.get(current)?;
            if node.is_class_like() {
                return Some(current);
            }
            current = self.arena.get_extended(current)?.parent;
        }
        None
    }

    fn is_private_or_protected(&self, node: &tsz_parser::parser::node::Node) -> bool {
        self.has_modifier(
            node,
            &[SyntaxKind::PrivateKeyword, SyntaxKind::ProtectedKeyword],
        )
    }

    fn is_static_member(&self, node: &tsz_parser::parser::node::Node) -> bool {
        self.has_modifier(node, &[SyntaxKind::StaticKeyword])
    }

    fn has_modifier(&self, node: &tsz_parser::parser::node::Node, keywords: &[SyntaxKind]) -> bool {
        let modifiers = match node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                .arena
                .get_property_decl(node)
                .and_then(|d| d.modifiers.as_ref()),
            k if k == syntax_kind_ext::METHOD_DECLARATION => self
                .arena
                .get_function(node)
                .and_then(|d| d.modifiers.as_ref()),
            _ => None,
        };
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    for kw in keywords {
                        if mod_node.kind == *kw as u16 {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    fn build_implements_edit(
        &self,
        class_idx: NodeIndex,
        class_data: &tsz_parser::parser::node::ClassData,
        interface_name: &str,
    ) -> Option<TextEdit> {
        // Check if there's already an implements clause
        if let Some(heritage_clauses) = &class_data.heritage_clauses {
            for &clause_idx in &heritage_clauses.nodes {
                let clause_node = self.arena.get(clause_idx)?;
                let heritage = self.arena.get_heritage_clause(clause_node)?;
                if heritage.token == SyntaxKind::ImplementsKeyword as u16 {
                    // Already has implements - add to existing
                    let clause_end = clause_node.end;
                    let pos = self.line_map.offset_to_position(clause_end, self.source);
                    return Some(TextEdit {
                        range: Range::new(pos, pos),
                        new_text: format!(", {interface_name}"),
                    });
                }
            }

            // Has heritage clauses but no implements - add after extends
            if let Some(&last_clause) = heritage_clauses.nodes.last() {
                let last_node = self.arena.get(last_clause)?;
                let pos = self.line_map.offset_to_position(last_node.end, self.source);
                return Some(TextEdit {
                    range: Range::new(pos, pos),
                    new_text: format!(" implements {interface_name}"),
                });
            }
        }

        // No heritage clauses at all - find the opening brace of the class
        let class_node = self.arena.get(class_idx)?;
        let class_text = self
            .source
            .get(class_node.pos as usize..class_node.end as usize)?;
        let brace_rel = class_text.find('{')?;
        let brace_offset = class_node.pos + brace_rel as u32;
        let pos = self.line_map.offset_to_position(brace_offset, self.source);
        Some(TextEdit {
            range: Range::new(pos, pos),
            new_text: format!("implements {interface_name} "),
        })
    }
}
