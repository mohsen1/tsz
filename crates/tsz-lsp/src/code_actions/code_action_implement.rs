//! Implement Interface / Override Method code actions.
//!
//! When the cursor is on a class that implements an interface or extends
//! a base class, these actions generate stub implementations for all
//! missing members.

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
    /// Generate stub implementations for all interface members that a class
    /// doesn't yet implement.
    ///
    /// Triggered when the cursor is on a class declaration that has an
    /// `implements` clause.
    pub fn implement_interface(&self, root: NodeIndex, range: Range) -> Option<CodeAction> {
        let start_offset = self.line_map.position_to_offset(range.start, self.source)?;
        let node_idx = find_node_at_offset(self.arena, start_offset);
        if node_idx.is_none() {
            return None;
        }

        // Find the class declaration
        let class_idx = self.find_ancestor_of_kind(node_idx, syntax_kind_ext::CLASS_DECLARATION)?;
        let class_node = self.arena.get(class_idx)?;
        let _class_data = self.arena.get_class(class_node)?;

        // Get existing class members
        let existing_members = self.collect_class_member_names(class_idx);

        // Find implemented interfaces from the heritage clause
        let interface_members = self.collect_interface_members_from_class(root, class_idx);
        if interface_members.is_empty() {
            return None;
        }

        // Find which members are missing
        let missing: Vec<&InterfaceMember> = interface_members
            .iter()
            .filter(|m| !existing_members.contains(&m.name))
            .collect();

        if missing.is_empty() {
            return None;
        }

        // Find insertion point (before closing brace of class body)
        let body_end = class_node.end.saturating_sub(1);
        let insert_pos = self.line_map.offset_to_position(body_end, self.source);

        // Get indentation
        let class_pos = self
            .line_map
            .offset_to_position(class_node.pos, self.source);
        let indent = self.get_indentation_at_position(&class_pos);
        let member_indent = format!("{indent}    ");

        // Generate stubs
        let mut new_text = String::new();
        for member in &missing {
            new_text.push('\n');
            match member.kind {
                MemberKind::Method => {
                    let params = member.params.as_deref().unwrap_or("");
                    let return_type = member
                        .return_type
                        .as_deref()
                        .map_or(String::new(), |rt| format!(": {rt}"));
                    new_text.push_str(&format!(
                        "{member_indent}{name}({params}){return_type} {{\n{member_indent}    throw new Error(\"Method not implemented.\");\n{member_indent}}}",
                        name = member.name
                    ));
                }
                MemberKind::Property => {
                    let type_ann = member
                        .return_type
                        .as_deref()
                        .map_or(String::new(), |t| format!(": {t}"));
                    new_text.push_str(&format!(
                        "{member_indent}{name}{type_ann};",
                        name = member.name
                    ));
                }
            }
            new_text.push('\n');
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
            title: format!("Implement {} missing member(s)", missing.len()),
            kind: CodeActionKind::QuickFix,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: true,
            data: None,
        })
    }

    /// Generate override stubs for abstract methods from a base class.
    ///
    /// Triggered when the cursor is on a class declaration that extends
    /// another class with abstract members.
    pub fn override_methods(&self, root: NodeIndex, range: Range) -> Option<CodeAction> {
        let start_offset = self.line_map.position_to_offset(range.start, self.source)?;
        let node_idx = find_node_at_offset(self.arena, start_offset);
        if node_idx.is_none() {
            return None;
        }

        // Find the class declaration
        let class_idx = self.find_ancestor_of_kind(node_idx, syntax_kind_ext::CLASS_DECLARATION)?;
        let class_node = self.arena.get(class_idx)?;
        let _class_data = self.arena.get_class(class_node)?;

        // Get existing class members
        let existing_members = self.collect_class_member_names(class_idx);

        // Find base class abstract members
        let base_members = self.collect_base_class_abstract_members(root, class_idx);
        if base_members.is_empty() {
            return None;
        }

        // Filter to missing abstract members
        let missing: Vec<&InterfaceMember> = base_members
            .iter()
            .filter(|m| !existing_members.contains(&m.name))
            .collect();

        if missing.is_empty() {
            return None;
        }

        // Find insertion point
        let body_end = class_node.end.saturating_sub(1);
        let insert_pos = self.line_map.offset_to_position(body_end, self.source);

        // Get indentation
        let class_pos = self
            .line_map
            .offset_to_position(class_node.pos, self.source);
        let indent = self.get_indentation_at_position(&class_pos);
        let member_indent = format!("{indent}    ");

        // Generate stubs
        let mut new_text = String::new();
        for member in &missing {
            new_text.push('\n');
            let params = member.params.as_deref().unwrap_or("");
            let return_type = member
                .return_type
                .as_deref()
                .map_or(String::new(), |rt| format!(": {rt}"));
            new_text.push_str(&format!(
                "{member_indent}override {name}({params}){return_type} {{\n{member_indent}    throw new Error(\"Method not implemented.\");\n{member_indent}}}",
                name = member.name
            ));
            new_text.push('\n');
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
            title: format!("Override {} abstract member(s)", missing.len()),
            kind: CodeActionKind::QuickFix,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: true,
            data: None,
        })
    }

    /// Collect names of existing members in a class.
    fn collect_class_member_names(&self, class_idx: NodeIndex) -> Vec<String> {
        let mut names = Vec::new();
        let class_node = match self.arena.get(class_idx) {
            Some(n) => n,
            None => return names,
        };

        // Scan children for method/property declarations
        for (i, node) in self.arena.nodes.iter().enumerate() {
            let idx = NodeIndex(i as u32);
            if node.pos < class_node.pos || node.pos > class_node.end {
                continue;
            }
            let parent = self
                .arena
                .get_extended(idx)
                .map_or(NodeIndex::NONE, |ext| ext.parent);

            // We want direct children of the class body
            if parent.is_none() {
                continue;
            }

            match node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(method) = self.arena.get_method_decl(node) {
                        if let Some(name) = self.arena.get_identifier_text(method.name) {
                            names.push(name.to_string());
                        }
                    }
                }
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    if let Some(prop) = self.arena.get_property_decl(node) {
                        if let Some(name) = self.arena.get_identifier_text(prop.name) {
                            names.push(name.to_string());
                        }
                    }
                }
                _ => {}
            }
        }

        names
    }

    /// Collect interface members by scanning the class node for heritage identifiers.
    fn collect_interface_members_from_class(
        &self,
        root: NodeIndex,
        class_idx: NodeIndex,
    ) -> Vec<InterfaceMember> {
        let mut members = Vec::new();
        let class_node = match self.arena.get(class_idx) {
            Some(n) => n,
            None => return members,
        };

        // Scan identifiers in the class header area (before the body)
        for (i, node) in self.arena.nodes.iter().enumerate() {
            let idx = NodeIndex(i as u32);
            if node.pos < class_node.pos || node.pos > class_node.end {
                continue;
            }
            if node.kind != SyntaxKind::Identifier as u16 {
                continue;
            }

            let mut walker = ScopeWalker::new(self.arena, self.binder);
            if let Some(symbol_id) = walker.resolve_node(root, idx) {
                if let Some(symbol) = self.binder.symbols.get(symbol_id) {
                    for &decl_idx in &symbol.declarations {
                        if let Some(decl_node) = self.arena.get(decl_idx) {
                            if decl_node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
                                self.collect_interface_declaration_members(decl_idx, &mut members);
                            }
                        }
                    }
                }
            }
        }

        members
    }

    /// Collect base class abstract members by scanning the class heritage.
    fn collect_base_class_abstract_members(
        &self,
        root: NodeIndex,
        class_idx: NodeIndex,
    ) -> Vec<InterfaceMember> {
        let mut members = Vec::new();
        let class_node = match self.arena.get(class_idx) {
            Some(n) => n,
            None => return members,
        };

        for (i, node) in self.arena.nodes.iter().enumerate() {
            let idx = NodeIndex(i as u32);
            if node.pos < class_node.pos || node.pos > class_node.end {
                continue;
            }
            if node.kind != SyntaxKind::Identifier as u16 {
                continue;
            }

            let mut walker = ScopeWalker::new(self.arena, self.binder);
            if let Some(symbol_id) = walker.resolve_node(root, idx) {
                if let Some(symbol) = self.binder.symbols.get(symbol_id) {
                    for &decl_idx in &symbol.declarations {
                        if let Some(decl_node) = self.arena.get(decl_idx) {
                            if decl_node.kind == syntax_kind_ext::CLASS_DECLARATION {
                                self.collect_abstract_class_members(decl_idx, &mut members);
                            }
                        }
                    }
                }
            }
        }

        members
    }

    /// Collect members from an interface declaration.
    fn collect_interface_declaration_members(
        &self,
        iface_idx: NodeIndex,
        members: &mut Vec<InterfaceMember>,
    ) {
        let iface_node = match self.arena.get(iface_idx) {
            Some(n) => n,
            None => return,
        };

        for (i, node) in self.arena.nodes.iter().enumerate() {
            let idx = NodeIndex(i as u32);
            if node.pos < iface_node.pos || node.pos > iface_node.end {
                continue;
            }

            let parent = self
                .arena
                .get_extended(idx)
                .map_or(NodeIndex::NONE, |ext| ext.parent);
            if parent.is_none() {
                continue;
            }

            match node.kind {
                k if k == syntax_kind_ext::METHOD_SIGNATURE => {
                    if let Some(method) = self.arena.get_method_decl(node) {
                        if let Some(name) = self.arena.get_identifier_text(method.name) {
                            let params_text = self.extract_params_text(idx);
                            let return_type = self.extract_return_type_text(idx);
                            members.push(InterfaceMember {
                                name: name.to_string(),
                                kind: MemberKind::Method,
                                params: Some(params_text),
                                return_type,
                            });
                        }
                    }
                }
                k if k == syntax_kind_ext::PROPERTY_SIGNATURE => {
                    if let Some(prop) = self.arena.get_property_decl(node) {
                        if let Some(name) = self.arena.get_identifier_text(prop.name) {
                            let type_text = self.extract_type_annotation_text(idx);
                            members.push(InterfaceMember {
                                name: name.to_string(),
                                kind: MemberKind::Property,
                                params: None,
                                return_type: type_text,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Collect abstract members from a class declaration.
    fn collect_abstract_class_members(
        &self,
        class_idx: NodeIndex,
        members: &mut Vec<InterfaceMember>,
    ) {
        let class_node = match self.arena.get(class_idx) {
            Some(n) => n,
            None => return,
        };

        for (i, node) in self.arena.nodes.iter().enumerate() {
            let idx = NodeIndex(i as u32);
            if node.pos < class_node.pos || node.pos > class_node.end {
                continue;
            }

            // Check if this member has the abstract modifier
            // We look for the `abstract` keyword as a child of the member
            if node.kind == syntax_kind_ext::METHOD_DECLARATION {
                if let Some(method) = self.arena.get_method_decl(node) {
                    if let Some(name) = self.arena.get_identifier_text(method.name) {
                        // Check for abstract modifier
                        if self.has_abstract_modifier(idx) {
                            let params_text = self.extract_params_text(idx);
                            let return_type = self.extract_return_type_text(idx);
                            members.push(InterfaceMember {
                                name: name.to_string(),
                                kind: MemberKind::Method,
                                params: Some(params_text),
                                return_type,
                            });
                        }
                    }
                }
            }
        }
    }

    /// Check if a member declaration has the `abstract` modifier.
    fn has_abstract_modifier(&self, member_idx: NodeIndex) -> bool {
        let member_node = match self.arena.get(member_idx) {
            Some(n) => n,
            None => return false,
        };

        for (i, node) in self.arena.nodes.iter().enumerate() {
            let idx = NodeIndex(i as u32);
            let parent = self
                .arena
                .get_extended(idx)
                .map_or(NodeIndex::NONE, |ext| ext.parent);

            if parent == member_idx && node.kind == SyntaxKind::AbstractKeyword as u16 {
                return true;
            }
            if node.pos > member_node.end {
                break;
            }
        }
        false
    }

    /// Extract parameters text from a method/function-like node.
    fn extract_params_text(&self, node_idx: NodeIndex) -> String {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return String::new(),
        };
        let text = match self.source.get(node.pos as usize..node.end as usize) {
            Some(t) => t,
            None => return String::new(),
        };

        // Extract text between first ( and matching )
        if let Some(open) = text.find('(') {
            let mut depth = 0;
            for (i, ch) in text[open..].char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            return text[open + 1..open + i].to_string();
                        }
                    }
                    _ => {}
                }
            }
        }
        String::new()
    }

    /// Extract return type text from a method/function-like node.
    fn extract_return_type_text(&self, node_idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(node_idx)?;
        let text = self.source.get(node.pos as usize..node.end as usize)?;

        // Find ): and extract the type after it
        if let Some(close_paren) = text.rfind(')') {
            let after = text[close_paren + 1..].trim();
            if let Some(stripped) = after.strip_prefix(':') {
                let type_text = stripped.trim().trim_end_matches(';').trim();
                if !type_text.is_empty() {
                    return Some(type_text.to_string());
                }
            }
        }
        None
    }

    /// Extract type annotation text from a property-like node.
    fn extract_type_annotation_text(&self, node_idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(node_idx)?;
        let text = self.source.get(node.pos as usize..node.end as usize)?;

        // Find the first : and extract the type
        if let Some(colon) = text.find(':') {
            let type_text = text[colon + 1..].trim().trim_end_matches(';').trim();
            if !type_text.is_empty() {
                return Some(type_text.to_string());
            }
        }
        None
    }
}

/// A member from an interface or abstract class.
struct InterfaceMember {
    name: String,
    kind: MemberKind,
    params: Option<String>,
    return_type: Option<String>,
}

enum MemberKind {
    Method,
    Property,
}
