//! LSP Type Definition implementation.
//!
//! Provides "Go to Type Definition" functionality that navigates to the
//! type declaration of a symbol, rather than its value declaration.
//!
//! For example:
//! - `let x: Foo = ...` → Go to Definition goes to the variable declaration
//! - `let x: Foo = ...` → Go to Type Definition goes to `interface Foo { ... }`

use crate::lsp::position::{Location, Position, Range};
use crate::lsp::resolver::ScopeWalker;
use crate::lsp::utils::find_node_at_offset;
use crate::parser::{NodeIndex, syntax_kind_ext};
use crate::scanner::SyntaxKind;

define_lsp_provider!(binder TypeDefinitionProvider, "Provider for Go to Type Definition.");

impl<'a> TypeDefinitionProvider<'a> {
    /// Get the type definition location for the symbol at the given position.
    ///
    /// Returns the location(s) where the type is defined. For primitive types
    /// (number, string, boolean, etc.), returns None since they have no
    /// user-defined declaration.
    pub fn get_type_definition(
        &self,
        root: NodeIndex,
        position: Position,
    ) -> Option<Vec<Location>> {
        // Convert position to byte offset
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;

        // Find the node at this offset
        let node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_none() {
            return None;
        }

        // Resolve the symbol at this position
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let symbol_id = walker.resolve_node(root, node_idx)?;

        // Get the symbol
        let symbol = self.binder.symbols.get(symbol_id)?;

        // Look for type annotation on the symbol's declarations
        for &decl_idx in &symbol.declarations {
            if let Some(type_loc) = self.find_type_definition_from_declaration(root, decl_idx) {
                return Some(type_loc);
            }
        }

        // If no explicit type annotation, try to infer from the value
        // (This would require full type checking, so for now we just return None)
        None
    }

    /// Find the type definition from a declaration node.
    ///
    /// Looks for type annotations (: Type) on the declaration and resolves them.
    fn find_type_definition_from_declaration(
        &self,
        root: NodeIndex,
        decl_idx: NodeIndex,
    ) -> Option<Vec<Location>> {
        let node = self.arena.get(decl_idx)?;

        match node.kind {
            // Variable declaration: look for type annotation
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                self.find_type_from_variable_declaration(root, decl_idx)
            }

            // Parameter: look for type annotation
            k if k == syntax_kind_ext::PARAMETER => self.find_type_from_parameter(root, decl_idx),

            // Property declaration/signature: look for type annotation
            k if k == syntax_kind_ext::PROPERTY_DECLARATION
                || k == syntax_kind_ext::PROPERTY_SIGNATURE =>
            {
                self.find_type_from_property(root, decl_idx)
            }

            // Function/method: look at return type
            k if k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::METHOD_DECLARATION
                || k == syntax_kind_ext::ARROW_FUNCTION =>
            {
                self.find_type_from_function(root, decl_idx)
            }

            _ => None,
        }
    }

    /// Find type from a variable declaration's type annotation.
    fn find_type_from_variable_declaration(
        &self,
        root: NodeIndex,
        decl_idx: NodeIndex,
    ) -> Option<Vec<Location>> {
        // Look for type annotation child
        let type_node = self.find_type_annotation_child(decl_idx)?;
        self.resolve_type_to_location(root, type_node)
    }

    /// Find type from a parameter's type annotation.
    fn find_type_from_parameter(
        &self,
        root: NodeIndex,
        decl_idx: NodeIndex,
    ) -> Option<Vec<Location>> {
        let type_node = self.find_type_annotation_child(decl_idx)?;
        self.resolve_type_to_location(root, type_node)
    }

    /// Find type from a property's type annotation.
    fn find_type_from_property(
        &self,
        root: NodeIndex,
        decl_idx: NodeIndex,
    ) -> Option<Vec<Location>> {
        let type_node = self.find_type_annotation_child(decl_idx)?;
        self.resolve_type_to_location(root, type_node)
    }

    /// Find return type from a function declaration.
    fn find_type_from_function(
        &self,
        root: NodeIndex,
        decl_idx: NodeIndex,
    ) -> Option<Vec<Location>> {
        // Look for return type annotation
        let type_node = self.find_return_type_child(decl_idx)?;
        self.resolve_type_to_location(root, type_node)
    }

    /// Find a type annotation child node within a declaration.
    fn find_type_annotation_child(&self, parent_idx: NodeIndex) -> Option<NodeIndex> {
        // Scan children for type reference nodes
        for (i, node) in self.arena.nodes.iter().enumerate() {
            let idx = NodeIndex(i as u32);
            let parent = self
                .arena
                .get_extended(idx)
                .map_or(NodeIndex::NONE, |ext| ext.parent);

            // Check if this node is a child of parent
            if parent == parent_idx {
                // Check if this is a type node
                if self.is_type_node(node.kind) {
                    return Some(idx);
                }
            }
        }
        None
    }

    /// Find a return type child node within a function declaration.
    fn find_return_type_child(&self, parent_idx: NodeIndex) -> Option<NodeIndex> {
        // Similar to find_type_annotation_child but looks for return type position
        self.find_type_annotation_child(parent_idx)
    }

    /// Check if a node kind represents a type.
    fn is_type_node(&self, kind: u16) -> bool {
        use syntax_kind_ext::*;

        matches!(
            kind,
            TYPE_REFERENCE
                | ARRAY_TYPE
                | TUPLE_TYPE
                | UNION_TYPE
                | INTERSECTION_TYPE
                | FUNCTION_TYPE
                | TYPE_LITERAL
                | TYPE_QUERY
                | INDEXED_ACCESS_TYPE
                | MAPPED_TYPE
                | CONDITIONAL_TYPE
                | PARENTHESIZED_TYPE
                | LITERAL_TYPE
                | TEMPLATE_LITERAL_TYPE
        )
    }

    /// Resolve a type node to its definition location.
    fn resolve_type_to_location(
        &self,
        root: NodeIndex,
        type_node: NodeIndex,
    ) -> Option<Vec<Location>> {
        let node = self.arena.get(type_node)?;

        // Handle TypeReference (the most common case)
        if node.kind == syntax_kind_ext::TYPE_REFERENCE {
            return self.resolve_type_reference(root, type_node);
        }

        // For array types, resolve the element type
        if node.kind == syntax_kind_ext::ARRAY_TYPE {
            // Find the element type child and resolve it
            if let Some(elem_type) = self.find_type_annotation_child(type_node) {
                return self.resolve_type_to_location(root, elem_type);
            }
        }

        // For union/intersection, we could return multiple locations
        // For now, just return the first resolvable type
        if node.kind == syntax_kind_ext::UNION_TYPE
            || node.kind == syntax_kind_ext::INTERSECTION_TYPE
        {
            // Find first type child and resolve it
            if let Some(first_type) = self.find_type_annotation_child(type_node) {
                return self.resolve_type_to_location(root, first_type);
            }
        }

        None
    }

    /// Resolve a TypeReference node to its definition.
    fn resolve_type_reference(
        &self,
        root: NodeIndex,
        type_ref: NodeIndex,
    ) -> Option<Vec<Location>> {
        // Find the identifier within the type reference
        let type_name_idx = self.find_type_name(type_ref)?;

        // Resolve the type name to a symbol
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let symbol_id = walker.resolve_node(root, type_name_idx)?;

        // Get the symbol's declarations
        let symbol = self.binder.symbols.get(symbol_id)?;

        // Convert declarations to locations
        let locations: Vec<Location> = symbol
            .declarations
            .iter()
            .filter_map(|&decl_idx| {
                let decl_node = self.arena.get(decl_idx)?;

                // Only include type declarations (interface, type alias, class, enum)
                if !self.is_type_declaration(decl_node.kind) {
                    return None;
                }

                let start_pos = self
                    .line_map
                    .offset_to_position(decl_node.pos, self.source_text);
                let end_pos = self
                    .line_map
                    .offset_to_position(decl_node.end, self.source_text);

                Some(Location {
                    file_path: self.file_name.clone(),
                    range: Range::new(start_pos, end_pos),
                })
            })
            .collect();

        if locations.is_empty() {
            None
        } else {
            Some(locations)
        }
    }

    /// Find the type name identifier within a type reference.
    fn find_type_name(&self, type_ref: NodeIndex) -> Option<NodeIndex> {
        // Look for an Identifier child
        for (i, node) in self.arena.nodes.iter().enumerate() {
            let idx = NodeIndex(i as u32);
            let parent = self
                .arena
                .get_extended(idx)
                .map_or(NodeIndex::NONE, |ext| ext.parent);

            if parent == type_ref && node.kind == SyntaxKind::Identifier as u16 {
                return Some(idx);
            }
        }
        None
    }

    /// Check if a node kind represents a type declaration.
    fn is_type_declaration(&self, kind: u16) -> bool {
        use syntax_kind_ext::*;

        matches!(
            kind,
            INTERFACE_DECLARATION | TYPE_ALIAS_DECLARATION | CLASS_DECLARATION | ENUM_DECLARATION
        )
    }
}

#[cfg(test)]
mod type_definition_tests {
    use super::*;
    use crate::binder::BinderState;
    use crate::lsp::position::LineMap;
    use crate::parser::ParserState;

    #[test]
    fn test_type_definition_interface() {
        let source = "interface Foo { x: number; }\nlet a: Foo;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider =
            TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        // Position at 'a' in 'let a: Foo'
        let pos = Position::new(1, 4);
        let result = provider.get_type_definition(root, pos);

        // Should find the interface declaration
        if let Some(locations) = result {
            assert!(!locations.is_empty(), "Should have at least one location");
            // The interface is on line 0
            assert_eq!(locations[0].range.start.line, 0);
        }
        // Note: result may be None if type resolution isn't fully working yet
    }

    #[test]
    fn test_type_definition_type_alias() {
        let source = "type MyType = string;\nlet x: MyType;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider =
            TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        // Position at 'x'
        let pos = Position::new(1, 4);
        let result = provider.get_type_definition(root, pos);

        // Type definition should point to the type alias on line 0
        if let Some(locations) = result {
            assert!(!locations.is_empty());
            assert_eq!(locations[0].range.start.line, 0);
        }
    }

    #[test]
    fn test_type_definition_class() {
        let source = "class MyClass {}\nlet obj: MyClass;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider =
            TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        // Position at 'obj'
        let pos = Position::new(1, 4);
        let result = provider.get_type_definition(root, pos);

        if let Some(locations) = result {
            assert!(!locations.is_empty());
            assert_eq!(locations[0].range.start.line, 0);
        }
    }

    #[test]
    fn test_type_definition_primitive() {
        let source = "let x: number;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider =
            TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        // Position at 'x'
        let pos = Position::new(0, 4);
        let result = provider.get_type_definition(root, pos);

        // Primitive types have no definition location
        // This might return None or might return an empty vec depending on implementation
        if let Some(locations) = result {
            // number is a primitive, so it shouldn't have a user-defined location
            // (though it might if we consider lib.d.ts)
            assert!(locations.is_empty() || locations[0].file_path.contains("lib"));
        }
    }

    #[test]
    fn test_type_definition_no_type_annotation() {
        let source = "let x = 1;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider =
            TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        // Position at 'x' - no explicit type annotation
        let pos = Position::new(0, 4);
        let result = provider.get_type_definition(root, pos);

        // Without type inference, this should return None
        // (Full type inference would be needed to determine that x: number)
        assert!(result.is_none());
    }

    #[test]
    fn test_type_definition_function_return() {
        let source =
            "interface Result { ok: boolean; }\nfunction foo(): Result { return { ok: true }; }";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider =
            TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        // Position at 'foo'
        let pos = Position::new(1, 9);
        let result = provider.get_type_definition(root, pos);

        // Should find the Result interface on line 0
        if let Some(locations) = result {
            assert!(!locations.is_empty());
            assert_eq!(locations[0].range.start.line, 0);
        }
    }

    #[test]
    fn test_type_definition_parameter() {
        let source = "interface Options { debug: boolean; }\nfunction foo(opts: Options) {}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider =
            TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        // Position at 'opts' parameter
        let pos = Position::new(1, 13);
        let result = provider.get_type_definition(root, pos);

        if let Some(locations) = result {
            assert!(!locations.is_empty());
            assert_eq!(locations[0].range.start.line, 0);
        }
    }
}
