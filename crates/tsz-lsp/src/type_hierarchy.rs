//! Type Hierarchy implementation for LSP.
//!
//! Provides type hierarchy navigation that shows supertypes and subtypes
//! for a given class or interface declaration:
//! - `prepare`: identifies the class/interface at a cursor position
//! - `supertypes`: finds what the class/interface extends or implements
//! - `subtypes`: finds what classes/interfaces extend or implement this type

use crate::document_symbols::SymbolKind;
use crate::utils::find_node_at_offset;
use tsz_binder::BinderState;
use tsz_common::position::{LineMap, Position, Range};
use tsz_parser::parser::node::NodeArena;
use tsz_parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

/// An item in the type hierarchy (represents a class or interface).
#[derive(Debug, Clone, serde::Serialize)]
pub struct TypeHierarchyItem {
    /// The name of the class/interface.
    pub name: String,
    /// The kind of this symbol (Class or Interface).
    pub kind: SymbolKind,
    /// The URI of the file containing this symbol.
    pub uri: String,
    /// The range enclosing the entire declaration.
    pub range: Range,
    /// The range of the declaration name (selection range).
    pub selection_range: Range,
    /// Additional detail, e.g. "class" or "interface".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Provider for type hierarchy operations.
pub struct TypeHierarchyProvider<'a> {
    arena: &'a NodeArena,
    line_map: &'a LineMap,
    file_name: String,
    source_text: &'a str,
}

impl<'a> TypeHierarchyProvider<'a> {
    /// Create a new type hierarchy provider.
    pub fn new(
        arena: &'a NodeArena,
        _binder: &'a BinderState,
        line_map: &'a LineMap,
        file_name: String,
        source_text: &'a str,
    ) -> Self {
        Self {
            arena,
            line_map,
            file_name,
            source_text,
        }
    }

    /// Prepare a type hierarchy item at the given position.
    ///
    /// Finds the class or interface at the cursor and returns a
    /// `TypeHierarchyItem` describing it. Returns `None` if the cursor
    /// is not on a class or interface declaration name.
    pub fn prepare(&self, _root: NodeIndex, position: Position) -> Option<TypeHierarchyItem> {
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;

        let node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_none() {
            return None;
        }

        // Find the class or interface declaration at or around this node
        let decl_idx = self.find_type_declaration_at_or_around(node_idx)?;
        self.make_type_hierarchy_item(decl_idx)
    }

    /// Find all supertypes for the class/interface at the given position.
    ///
    /// Walks the heritage clauses (extends, implements) of the declaration
    /// to find parent types. For each parent type name found in a heritage
    /// clause, searches the file for its declaration and returns an item.
    pub fn supertypes(&self, _root: NodeIndex, position: Position) -> Vec<TypeHierarchyItem> {
        let mut results = Vec::new();

        let offset = match self.line_map.position_to_offset(position, self.source_text) {
            Some(o) => o,
            None => return results,
        };

        let node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_none() {
            return results;
        }

        let decl_idx = match self.find_type_declaration_at_or_around(node_idx) {
            Some(idx) => idx,
            None => return results,
        };

        // Collect supertype names from heritage clauses
        let supertype_names = self.collect_heritage_type_names(decl_idx);

        // For each supertype name, find its declaration in the file
        for name in &supertype_names {
            if let Some(item) = self.find_type_declaration_by_name(name) {
                results.push(item);
            }
        }

        results
    }

    /// Find all subtypes for the class/interface at the given position.
    ///
    /// Walks all class and interface declarations in the file, checking their
    /// heritage clauses to see if they reference the target type name.
    pub fn subtypes(&self, _root: NodeIndex, position: Position) -> Vec<TypeHierarchyItem> {
        let mut results = Vec::new();

        let offset = match self.line_map.position_to_offset(position, self.source_text) {
            Some(o) => o,
            None => return results,
        };

        let node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_none() {
            return results;
        }

        let decl_idx = match self.find_type_declaration_at_or_around(node_idx) {
            Some(idx) => idx,
            None => return results,
        };

        // Get the target type name
        let target_name = match self.get_declaration_name(decl_idx) {
            Some(name) => name,
            None => return results,
        };

        // Walk all nodes in the arena looking for class/interface declarations
        // that reference the target name in their heritage clauses
        for (i, node) in self.arena.nodes.iter().enumerate() {
            let candidate_idx = NodeIndex(i as u32);

            // Skip the declaration itself
            if candidate_idx == decl_idx {
                continue;
            }

            match node.kind {
                k if k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::CLASS_EXPRESSION =>
                {
                    if let Some(class) = self.arena.get_class(node)
                        && self
                            .heritage_clauses_reference_name(&class.heritage_clauses, &target_name)
                        && let Some(item) = self.make_type_hierarchy_item(candidate_idx)
                    {
                        results.push(item);
                    }
                }
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                    if let Some(iface) = self.arena.get_interface(node)
                        && self
                            .heritage_clauses_reference_name(&iface.heritage_clauses, &target_name)
                        && let Some(item) = self.make_type_hierarchy_item(candidate_idx)
                    {
                        results.push(item);
                    }
                }
                _ => {}
            }
        }

        results
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Check whether a node kind is a class or interface declaration.
    fn is_type_declaration(&self, kind: u16) -> bool {
        kind == syntax_kind_ext::CLASS_DECLARATION
            || kind == syntax_kind_ext::CLASS_EXPRESSION
            || kind == syntax_kind_ext::INTERFACE_DECLARATION
    }

    /// Find the class or interface declaration at or containing the given node.
    ///
    /// If the node itself is a class/interface declaration, return it.
    /// If the node is an identifier whose parent is a class/interface, return the parent.
    /// Otherwise walk up through parents.
    fn find_type_declaration_at_or_around(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        if node_idx.is_none() {
            return None;
        }

        let node = self.arena.get(node_idx)?;

        // If we are directly on a class/interface declaration, return it.
        if self.is_type_declaration(node.kind) {
            return Some(node_idx);
        }

        // If the node is an identifier, check if its parent is a class/interface
        // declaration (i.e., we are on the type name).
        if node.kind == SyntaxKind::Identifier as u16
            && let Some(ext) = self.arena.get_extended(node_idx)
        {
            let parent = ext.parent;
            if !parent.is_none()
                && let Some(parent_node) = self.arena.get(parent)
                && self.is_type_declaration(parent_node.kind)
            {
                return Some(parent);
            }
        }

        // Walk up through parents to find an enclosing class/interface.
        let mut current = node_idx;
        loop {
            let ext = self.arena.get_extended(current)?;
            let parent = ext.parent;
            if parent.is_none() {
                return None;
            }
            let parent_node = self.arena.get(parent)?;
            if self.is_type_declaration(parent_node.kind) {
                return Some(parent);
            }
            current = parent;
        }
    }

    /// Get the name of a class or interface declaration.
    fn get_declaration_name(&self, decl_idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(decl_idx)?;

        match node.kind {
            k if k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::CLASS_EXPRESSION =>
            {
                let class = self.arena.get_class(node)?;
                self.get_identifier_text(class.name)
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                let iface = self.arena.get_interface(node)?;
                self.get_identifier_text(iface.name)
            }
            _ => None,
        }
    }

    /// Get the name NodeIndex of a class or interface declaration.
    fn get_declaration_name_idx(&self, decl_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.arena.get(decl_idx)?;

        match node.kind {
            k if k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::CLASS_EXPRESSION =>
            {
                let class = self.arena.get_class(node)?;
                if class.name.is_none() {
                    None
                } else {
                    Some(class.name)
                }
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                let iface = self.arena.get_interface(node)?;
                if iface.name.is_none() {
                    None
                } else {
                    Some(iface.name)
                }
            }
            _ => None,
        }
    }

    /// Collect all type names referenced in the heritage clauses of a declaration.
    ///
    /// For a class, this includes both extends and implements clauses.
    /// For an interface, this includes extends clauses.
    fn collect_heritage_type_names(&self, decl_idx: NodeIndex) -> Vec<String> {
        let mut names = Vec::new();

        let node = match self.arena.get(decl_idx) {
            Some(n) => n,
            None => return names,
        };

        let heritage_clauses = match node.kind {
            k if k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::CLASS_EXPRESSION =>
            {
                self.arena
                    .get_class(node)
                    .and_then(|c| c.heritage_clauses.as_ref())
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => self
                .arena
                .get_interface(node)
                .and_then(|i| i.heritage_clauses.as_ref()),
            _ => None,
        };

        let heritage_clauses = match heritage_clauses {
            Some(hc) => hc,
            None => return names,
        };

        for &clause_idx in &heritage_clauses.nodes {
            let clause_node = match self.arena.get(clause_idx) {
                Some(n) => n,
                None => continue,
            };
            let heritage_data = match self.arena.get_heritage_clause(clause_node) {
                Some(d) => d,
                None => continue,
            };

            // Walk the types in this heritage clause
            for &type_idx in &heritage_data.types.nodes {
                let type_node = match self.arena.get(type_idx) {
                    Some(n) => n,
                    None => continue,
                };

                // Heritage clause types can be either:
                // 1. ExpressionWithTypeArguments nodes wrapping an identifier
                // 2. Plain Identifier nodes directly
                if let Some(expr_data) = self.arena.get_expr_type_args(type_node) {
                    if let Some(name) = self.extract_expression_name(expr_data.expression) {
                        names.push(name);
                    }
                } else if let Some(name) = self.extract_expression_name(type_idx) {
                    names.push(name);
                }
            }
        }

        names
    }

    /// Check if any heritage clause in the given list references the target name.
    fn heritage_clauses_reference_name(
        &self,
        heritage_clauses: &Option<tsz_parser::parser::base::NodeList>,
        target_name: &str,
    ) -> bool {
        let heritage_clauses = match heritage_clauses {
            Some(hc) => hc,
            None => return false,
        };

        for &clause_idx in &heritage_clauses.nodes {
            let clause_node = match self.arena.get(clause_idx) {
                Some(n) => n,
                None => continue,
            };
            let heritage_data = match self.arena.get_heritage_clause(clause_node) {
                Some(d) => d,
                None => continue,
            };

            if self.heritage_types_contain_name(&heritage_data.types, target_name) {
                return true;
            }
        }

        false
    }

    /// Check if a heritage clause's type list contains a reference to the target name.
    ///
    /// Heritage clause types can be either `ExpressionWithTypeArguments` nodes
    /// or plain `Identifier` nodes, depending on the parser implementation.
    /// For example: `implements Foo, Bar<T>` has two entries in the types list.
    fn heritage_types_contain_name(
        &self,
        types: &tsz_parser::parser::base::NodeList,
        target_name: &str,
    ) -> bool {
        for &type_idx in &types.nodes {
            let type_node = match self.arena.get(type_idx) {
                Some(n) => n,
                None => continue,
            };

            // Handle ExpressionWithTypeArguments wrapping an identifier
            if let Some(expr_data) = self.arena.get_expr_type_args(type_node) {
                if self.expression_matches_name(expr_data.expression, target_name) {
                    return true;
                }
            } else if self.expression_matches_name(type_idx, target_name) {
                // Handle plain Identifier or PropertyAccessExpression directly
                return true;
            }
        }
        false
    }

    /// Check if an expression node (typically an Identifier) matches the target name.
    /// Handles both simple identifiers and property access expressions (e.g., `Ns.Foo`).
    fn expression_matches_name(&self, expr_idx: NodeIndex, target_name: &str) -> bool {
        if expr_idx.is_none() {
            return false;
        }

        let expr_node = match self.arena.get(expr_idx) {
            Some(n) => n,
            None => return false,
        };

        // Simple identifier: `Foo`
        if expr_node.kind == SyntaxKind::Identifier as u16
            && let Some(text) = self.get_identifier_text(expr_idx)
        {
            return text == target_name;
        }

        // Property access: `Ns.Foo` - check the rightmost name
        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.arena.get_access_expr(expr_node)
        {
            return self.expression_matches_name(access.name_or_argument, target_name);
        }

        false
    }

    /// Extract the name from an expression node (for heritage clause type references).
    /// Returns the simple identifier text, or for property access, the full dotted name.
    fn extract_expression_name(&self, expr_idx: NodeIndex) -> Option<String> {
        if expr_idx.is_none() {
            return None;
        }

        let expr_node = self.arena.get(expr_idx)?;

        // Simple identifier: `Foo`
        if expr_node.kind == SyntaxKind::Identifier as u16 {
            return self.get_identifier_text(expr_idx);
        }

        // Property access: `Ns.Foo` - return the rightmost name
        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.arena.get_access_expr(expr_node)
        {
            return self.extract_expression_name(access.name_or_argument);
        }

        None
    }

    /// Find a class or interface declaration by name within the current file.
    ///
    /// Walks all nodes in the arena to find a class/interface with a matching name.
    fn find_type_declaration_by_name(&self, name: &str) -> Option<TypeHierarchyItem> {
        for (i, node) in self.arena.nodes.iter().enumerate() {
            let candidate_idx = NodeIndex(i as u32);

            match node.kind {
                k if k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::CLASS_EXPRESSION =>
                {
                    if let Some(class) = self.arena.get_class(node)
                        && let Some(text) = self.get_identifier_text(class.name)
                        && text == name
                    {
                        return self.make_type_hierarchy_item(candidate_idx);
                    }
                }
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                    if let Some(iface) = self.arena.get_interface(node)
                        && let Some(text) = self.get_identifier_text(iface.name)
                        && text == name
                    {
                        return self.make_type_hierarchy_item(candidate_idx);
                    }
                }
                _ => {}
            }
        }

        None
    }

    /// Build a `TypeHierarchyItem` for a class or interface declaration node.
    fn make_type_hierarchy_item(&self, decl_idx: NodeIndex) -> Option<TypeHierarchyItem> {
        let node = self.arena.get(decl_idx)?;
        if !self.is_type_declaration(node.kind) {
            return None;
        }

        let name = self.get_declaration_name(decl_idx)?;
        let kind = self.get_type_symbol_kind(decl_idx);
        let range = self.get_range(decl_idx);

        // Selection range is the name identifier range
        let selection_range = if let Some(name_idx) = self.get_declaration_name_idx(decl_idx) {
            self.get_range(name_idx)
        } else {
            // Fallback: use a small range at the start of the declaration
            let start = self.line_map.offset_to_position(node.pos, self.source_text);
            let end = self
                .line_map
                .offset_to_position(node.pos.saturating_add(5), self.source_text);
            Range::new(start, end)
        };

        let detail = match node.kind {
            k if k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::CLASS_EXPRESSION =>
            {
                Some("class".to_string())
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => Some("interface".to_string()),
            _ => None,
        };

        Some(TypeHierarchyItem {
            name,
            kind,
            uri: self.file_name.clone(),
            range,
            selection_range,
            detail,
        })
    }

    /// Get the SymbolKind for a class or interface declaration.
    fn get_type_symbol_kind(&self, decl_idx: NodeIndex) -> SymbolKind {
        let node = match self.arena.get(decl_idx) {
            Some(n) => n,
            None => return SymbolKind::Class,
        };

        match node.kind {
            k if k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::CLASS_EXPRESSION =>
            {
                SymbolKind::Class
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => SymbolKind::Interface,
            _ => SymbolKind::Class,
        }
    }

    /// Get the text of an identifier node.
    fn get_identifier_text(&self, node_idx: NodeIndex) -> Option<String> {
        if node_idx.is_none() {
            return None;
        }
        let node = self.arena.get(node_idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            self.arena
                .get_identifier(node)
                .map(|id| id.escaped_text.clone())
        } else {
            None
        }
    }

    /// Convert a node to an LSP Range.
    fn get_range(&self, node_idx: NodeIndex) -> Range {
        if let Some(node) = self.arena.get(node_idx) {
            let start = self.line_map.offset_to_position(node.pos, self.source_text);
            let end = self.line_map.offset_to_position(node.end, self.source_text);
            Range::new(start, end)
        } else {
            Range::new(Position::new(0, 0), Position::new(0, 0))
        }
    }
}

#[cfg(test)]
#[path = "../tests/type_hierarchy_tests.rs"]
mod type_hierarchy_tests;
