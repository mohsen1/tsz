//! Go to Implementation for LSP.
//!
//! Given a position in the source at an interface or abstract class declaration,
//! finds all concrete implementations of that type within the current file.
//!
//! Strategy:
//! 1. Find the symbol at cursor position (reuse GoToDefinition pattern)
//! 2. Determine if it's an interface or abstract class
//! 3. Walk all ClassDeclaration / InterfaceDeclaration nodes in the AST
//! 4. For each class/interface, check heritage clauses for the target name
//! 5. Return locations of implementing classes/interfaces

use crate::lsp::position::{Location, Position, Range};
use crate::lsp::utils::find_node_at_offset;
use crate::parser::{NodeIndex, modifier_flags, syntax_kind_ext};
use crate::scanner::SyntaxKind;

define_lsp_provider!(binder GoToImplementationProvider, "Go to Implementation provider.");

/// The kind of target the user is searching implementations for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    /// An interface: look for classes that `implements` it or interfaces that `extends` it.
    Interface,
    /// An abstract class: look for classes that `extends` it.
    AbstractClass,
    /// A concrete class: look for classes that `extends` it.
    ConcreteClass,
}

/// Result of finding an implementation: the class/interface name and its location.
#[derive(Debug, Clone)]
pub struct ImplementationResult {
    /// The name of the implementing class/interface
    pub name: String,
    /// The location of the implementation declaration
    pub location: Location,
}

impl<'a> GoToImplementationProvider<'a> {
    /// Get the escaped_text for an identifier node, reading directly from IdentifierData.
    ///
    /// This bypasses the interner-based `get_identifier_text` which requires the interner
    /// to be transferred from the scanner to the arena (done by `into_arena()` but not
    /// by `get_arena()`). The `escaped_text` field is always populated by the parser.
    fn get_identifier_escaped_text(&self, node_idx: NodeIndex) -> Option<&str> {
        let node = self.arena.get(node_idx)?;
        let data = self.arena.get_identifier(node)?;
        Some(&data.escaped_text)
    }

    /// Resolve the symbol at a node index.
    ///
    /// Tries multiple strategies:
    /// 1. Direct lookup in `node_symbols` (works for declaration nodes)
    /// 2. Parent lookup in `node_symbols` (works for name identifiers of declarations)
    /// 3. Name-based lookup in `file_locals` using escaped_text
    pub fn resolve_symbol_at_node(&self, node_idx: NodeIndex) -> Option<crate::binder::SymbolId> {
        // Strategy 1: Direct lookup - the node itself is a declaration node
        if let Some(&sym_id) = self.binder.node_symbols.get(&node_idx.0) {
            return Some(sym_id);
        }

        // Strategy 2: Parent lookup - the node is the name identifier of a declaration
        if let Some(ext) = self.arena.get_extended(node_idx) {
            if let Some(&sym_id) = self.binder.node_symbols.get(&ext.parent.0) {
                return Some(sym_id);
            }
        }

        // Strategy 3: Name-based lookup in file_locals using escaped_text
        let node = self.arena.get(node_idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            let text = self.get_identifier_escaped_text(node_idx)?;
            if !text.is_empty() {
                return self.binder.file_locals.get(text);
            }
        }

        None
    }

    /// Get the implementation locations for the symbol at the given position.
    ///
    /// Returns a list of locations where the interface/abstract class is implemented.
    /// Returns None if no symbol is found at the position or the symbol is not an
    /// interface or abstract class.
    pub fn get_implementations(
        &self,
        root: NodeIndex,
        position: Position,
    ) -> Option<Vec<Location>> {
        // 1. Convert position to byte offset
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;

        // 2. Find the most specific node at this offset
        let node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_none() {
            return None;
        }

        // 3. Resolve the node to a symbol
        let symbol_id = self.resolve_symbol_at_node(node_idx)?;

        // 4. Get the symbol and determine its kind
        let symbol = self.binder.symbols.get(symbol_id)?;
        let target_name = symbol.escaped_name.clone();

        // Determine if the target is an interface or a class (abstract or not)
        let target_kind = self.determine_target_kind(symbol)?;

        // 5. Collect all implementing declarations
        let mut locations = Vec::new();
        self.collect_implementations(root, &target_name, target_kind, &mut locations);

        if locations.is_empty() {
            None
        } else {
            Some(locations)
        }
    }

    /// Determine if a symbol represents an interface, abstract class, or concrete class.
    pub fn determine_target_kind(&self, symbol: &crate::binder::Symbol) -> Option<TargetKind> {
        use crate::binder::symbol_flags;

        if symbol.flags & symbol_flags::INTERFACE != 0 {
            return Some(TargetKind::Interface);
        }

        if symbol.flags & symbol_flags::CLASS != 0 {
            // Check if the class is abstract by examining its declarations
            for &decl_idx in &symbol.declarations {
                if let Some(ext) = self.arena.get_extended(decl_idx) {
                    if ext.modifier_flags & modifier_flags::ABSTRACT != 0 {
                        return Some(TargetKind::AbstractClass);
                    }
                }
            }
            return Some(TargetKind::ConcreteClass);
        }

        None
    }

    /// Walk all nodes in the arena to find classes/interfaces that implement or extend the target.
    fn collect_implementations(
        &self,
        _root: NodeIndex,
        target_name: &str,
        target_kind: TargetKind,
        locations: &mut Vec<Location>,
    ) {
        // Iterate over all nodes in the arena looking for class and interface declarations
        for (i, node) in self.arena.nodes.iter().enumerate() {
            let node_idx = NodeIndex(i as u32);

            match node.kind {
                k if k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::CLASS_EXPRESSION =>
                {
                    if let Some(class) = self.arena.get_class(node) {
                        if self.class_implements_or_extends(class, target_name, target_kind) {
                            if let Some(loc) = self.location_for_declaration(node_idx, class.name) {
                                locations.push(loc);
                            }
                        }
                    }
                }
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                    // An interface can extend another interface
                    if target_kind == TargetKind::Interface {
                        if let Some(iface) = self.arena.get_interface(node) {
                            if self.interface_extends(iface, target_name) {
                                if let Some(loc) =
                                    self.location_for_declaration(node_idx, iface.name)
                                {
                                    locations.push(loc);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Check if a class implements or extends the target name.
    fn class_implements_or_extends(
        &self,
        class: &crate::parser::node::ClassData,
        target_name: &str,
        target_kind: TargetKind,
    ) -> bool {
        let Some(ref heritage) = class.heritage_clauses else {
            return false;
        };

        for &clause_idx in &heritage.nodes {
            let Some(clause_node) = self.arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage_data) = self.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            let token = heritage_data.token;
            let is_implements = token == SyntaxKind::ImplementsKeyword as u16;
            let is_extends = token == SyntaxKind::ExtendsKeyword as u16;

            // For interfaces: look for `implements InterfaceName`
            // For abstract/concrete classes: look for `extends ClassName`
            let should_check = match target_kind {
                TargetKind::Interface => is_implements,
                TargetKind::AbstractClass | TargetKind::ConcreteClass => is_extends,
            };

            if !should_check {
                continue;
            }

            if self.heritage_types_contain_name(&heritage_data.types, target_name) {
                return true;
            }
        }

        false
    }

    /// Check if an interface extends the target name.
    fn interface_extends(
        &self,
        iface: &crate::parser::node::InterfaceData,
        target_name: &str,
    ) -> bool {
        let Some(ref heritage) = iface.heritage_clauses else {
            return false;
        };

        for &clause_idx in &heritage.nodes {
            let Some(clause_node) = self.arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage_data) = self.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            // Interfaces use `extends` to inherit from other interfaces
            if heritage_data.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }

            if self.heritage_types_contain_name(&heritage_data.types, target_name) {
                return true;
            }
        }

        false
    }

    /// Check if a heritage clause's type list contains a reference to the target name.
    ///
    /// Heritage clause types may be `ExpressionWithTypeArguments` nodes (wrapping an
    /// expression child) or bare Identifier/expression nodes. For example:
    ///   `implements Foo, Bar<T>` has two entries in the types list.
    fn heritage_types_contain_name(
        &self,
        types: &crate::parser::base::NodeList,
        target_name: &str,
    ) -> bool {
        for &type_idx in &types.nodes {
            let Some(type_node) = self.arena.get(type_idx) else {
                continue;
            };

            // Case 1: ExpressionWithTypeArguments wraps an expression child
            if let Some(expr_data) = self.arena.get_expr_type_args(type_node) {
                if self.expression_matches_name(expr_data.expression, target_name) {
                    return true;
                }
            }

            // Case 2: The type entry is directly an Identifier or expression node
            if self.expression_matches_name(type_idx, target_name) {
                return true;
            }
        }
        false
    }

    /// Check if an expression node (typically an Identifier) matches the target name.
    /// Handles both simple identifiers and property access expressions (e.g., `Ns.Foo`).
    ///
    /// Uses `escaped_text` directly from IdentifierData to avoid depending on the interner.
    fn expression_matches_name(&self, expr_idx: NodeIndex, target_name: &str) -> bool {
        if expr_idx.is_none() {
            return false;
        }

        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };

        // Simple identifier: `Foo`
        if expr_node.kind == SyntaxKind::Identifier as u16 {
            if let Some(text) = self.get_identifier_escaped_text(expr_idx) {
                return text == target_name;
            }
        }

        // Property access: `Ns.Foo` - check the rightmost name
        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            if let Some(access) = self.arena.get_access_expr(expr_node) {
                return self.expression_matches_name(access.name_or_argument, target_name);
            }
        }

        false
    }

    /// Create a Location for a declaration node, preferring the name node for a tighter range.
    fn location_for_declaration(
        &self,
        decl_idx: NodeIndex,
        name_idx: NodeIndex,
    ) -> Option<Location> {
        // Use the name node if available for a tighter range
        let target_idx = if !name_idx.is_none() {
            name_idx
        } else {
            decl_idx
        };

        let node = self.arena.get(target_idx)?;
        let start_pos = self.line_map.offset_to_position(node.pos, self.source_text);
        let end_pos = self.line_map.offset_to_position(node.end, self.source_text);

        Some(Location {
            file_path: self.file_name.clone(),
            range: Range::new(start_pos, end_pos),
        })
    }

    /// Find implementations of a target by name (for project-wide search).
    ///
    /// This method is used by Project for cross-file implementation search.
    /// It searches the current file for classes/interfaces that implement or extend
    /// the given target name, returning both the locations and the implementing names
    /// (for transitive search).
    ///
    /// # Arguments
    /// * `target_name` - The name of the interface/class to find implementations for
    /// * `target_kind` - The kind of target (Interface, AbstractClass, or ConcreteClass)
    ///
    /// # Returns
    /// A vector of ImplementationResult containing the implementing class/interface names
    /// and their locations
    pub fn find_implementations_for_name(
        &self,
        target_name: &str,
        target_kind: TargetKind,
    ) -> Vec<ImplementationResult> {
        let mut results = Vec::new();

        // Iterate over all nodes in the arena looking for class and interface declarations
        for (i, node) in self.arena.nodes.iter().enumerate() {
            let node_idx = NodeIndex(i as u32);

            match node.kind {
                k if k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::CLASS_EXPRESSION =>
                {
                    if let Some(class) = self.arena.get_class(node) {
                        if self.class_implements_or_extends(class, target_name, target_kind) {
                            if let Some(class_name) = self.get_identifier_escaped_text(class.name) {
                                if let Some(loc) =
                                    self.location_for_declaration(node_idx, class.name)
                                {
                                    results.push(ImplementationResult {
                                        name: class_name.to_string(),
                                        location: loc,
                                    });
                                }
                            }
                        }
                    }
                }
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                    // An interface can extend another interface
                    if target_kind == TargetKind::Interface {
                        if let Some(iface) = self.arena.get_interface(node) {
                            if self.interface_extends(iface, target_name) {
                                if let Some(iface_name) =
                                    self.get_identifier_escaped_text(iface.name)
                                {
                                    if let Some(loc) =
                                        self.location_for_declaration(node_idx, iface.name)
                                    {
                                        results.push(ImplementationResult {
                                            name: iface_name.to_string(),
                                            location: loc,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        results
    }

    /// Resolve the target kind for a symbol by name.
    ///
    /// This is used by Project to determine what kind of target we're searching for
    /// when doing project-wide implementation search.
    ///
    /// # Arguments
    /// * `symbol_name` - The name of the symbol to resolve
    ///
    /// # Returns
    /// The TargetKind if the symbol is found and is an interface or class, None otherwise
    pub fn resolve_target_kind_for_name(&self, symbol_name: &str) -> Option<TargetKind> {
        // Look up the symbol by name in file_locals
        let symbol_id = self.binder.file_locals.get(symbol_name)?;
        let symbol = self.binder.symbols.get(symbol_id)?;
        self.determine_target_kind(symbol)
    }
}

#[cfg(test)]
mod implementation_tests {
    use super::*;
    use crate::binder::BinderState;
    use crate::lsp::position::LineMap;
    use crate::parser::ParserState;

    #[test]
    fn test_interface_single_implementor() {
        let source = "interface Animal {\n  speak(): void;\n}\nclass Dog implements Animal {\n  speak() {}\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        let provider = GoToImplementationProvider::new(
            arena,
            &binder,
            &line_map,
            "test.ts".to_string(),
            source,
        );

        // Position at "Animal" in "interface Animal" (line 0, col ~10)
        let pos = Position::new(0, 10);
        let result = provider.get_implementations(root, pos);

        assert!(result.is_some(), "Should find implementations for Animal");
        let locs = result.unwrap();
        assert_eq!(locs.len(), 1, "Should find exactly one implementor");
        // The implementing class "Dog" is on line 3
        assert_eq!(locs[0].range.start.line, 3);
    }

    #[test]
    fn test_interface_multiple_implementors() {
        let source = "interface Shape {\n  area(): number;\n}\nclass Circle implements Shape {\n  area() { return 0; }\n}\nclass Square implements Shape {\n  area() { return 0; }\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        let provider = GoToImplementationProvider::new(
            arena,
            &binder,
            &line_map,
            "test.ts".to_string(),
            source,
        );

        // Position at "Shape" in "interface Shape"
        let pos = Position::new(0, 10);
        let result = provider.get_implementations(root, pos);

        assert!(result.is_some(), "Should find implementations for Shape");
        let locs = result.unwrap();
        assert_eq!(locs.len(), 2, "Should find two implementors");
    }

    #[test]
    fn test_interface_extends_interface() {
        let source = "interface Base {\n  id: number;\n}\ninterface Extended extends Base {\n  name: string;\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        let provider = GoToImplementationProvider::new(
            arena,
            &binder,
            &line_map,
            "test.ts".to_string(),
            source,
        );

        // Position at "Base" in "interface Base"
        let pos = Position::new(0, 10);
        let result = provider.get_implementations(root, pos);

        assert!(result.is_some(), "Should find interfaces extending Base");
        let locs = result.unwrap();
        assert_eq!(locs.len(), 1, "Should find one extending interface");
        assert_eq!(locs[0].range.start.line, 3);
    }

    #[test]
    fn test_abstract_class_implementor() {
        let source = "abstract class Vehicle {\n  abstract drive(): void;\n}\nclass Car extends Vehicle {\n  drive() {}\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        let provider = GoToImplementationProvider::new(
            arena,
            &binder,
            &line_map,
            "test.ts".to_string(),
            source,
        );

        // Position at "Vehicle" in "abstract class Vehicle"
        let pos = Position::new(0, 15);
        let result = provider.get_implementations(root, pos);

        assert!(
            result.is_some(),
            "Should find implementations for abstract class Vehicle"
        );
        let locs = result.unwrap();
        assert_eq!(locs.len(), 1, "Should find one implementor");
        assert_eq!(locs[0].range.start.line, 3);
    }

    #[test]
    fn test_class_extends_concrete_class() {
        let source =
            "class Base {\n  method() {}\n}\nclass Derived extends Base {\n  method() {}\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        let provider = GoToImplementationProvider::new(
            arena,
            &binder,
            &line_map,
            "test.ts".to_string(),
            source,
        );

        // Position at "Base" in "class Base"
        let pos = Position::new(0, 6);
        let result = provider.get_implementations(root, pos);

        assert!(result.is_some(), "Should find subclasses of Base");
        let locs = result.unwrap();
        assert_eq!(locs.len(), 1, "Should find one subclass");
        assert_eq!(locs[0].range.start.line, 3);
    }

    #[test]
    fn test_no_implementations() {
        let source = "interface Lonely {\n  value: number;\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        let provider = GoToImplementationProvider::new(
            arena,
            &binder,
            &line_map,
            "test.ts".to_string(),
            source,
        );

        let pos = Position::new(0, 10);
        let result = provider.get_implementations(root, pos);

        assert!(
            result.is_none(),
            "Should return None when no implementations exist"
        );
    }

    #[test]
    fn test_not_on_interface_or_class() {
        let source = "const x = 1;\nx + 1;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        let provider = GoToImplementationProvider::new(
            arena,
            &binder,
            &line_map,
            "test.ts".to_string(),
            source,
        );

        // Position at "x" in "const x = 1"
        let pos = Position::new(0, 6);
        let result = provider.get_implementations(root, pos);

        assert!(
            result.is_none(),
            "Should return None for non-interface/class symbols"
        );
    }

    #[test]
    fn test_interface_with_multiple_heritage_types() {
        let source = "interface A {}\ninterface B {}\nclass C implements A, B {}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        let provider = GoToImplementationProvider::new(
            arena,
            &binder,
            &line_map,
            "test.ts".to_string(),
            source,
        );

        // Test that searching for A finds C
        let pos_a = Position::new(0, 10);
        let result_a = provider.get_implementations(root, pos_a);
        assert!(result_a.is_some(), "Should find implementors of A");
        assert_eq!(result_a.unwrap().len(), 1);

        // Test that searching for B also finds C
        let pos_b = Position::new(1, 10);
        let result_b = provider.get_implementations(root, pos_b);
        assert!(result_b.is_some(), "Should find implementors of B");
        assert_eq!(result_b.unwrap().len(), 1);
    }

    #[test]
    fn test_position_at_semicolon() {
        let source = "interface Foo {}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        let provider = GoToImplementationProvider::new(
            arena,
            &binder,
            &line_map,
            "test.ts".to_string(),
            source,
        );

        // Position past the end of the content
        let pos = Position::new(0, 50);
        let result = provider.get_implementations(root, pos);

        assert!(
            result.is_none(),
            "Should return None for position outside content"
        );
    }

    #[test]
    fn test_class_chain() {
        let source = "class A {}\nclass B extends A {}\nclass C extends B {}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        let provider = GoToImplementationProvider::new(
            arena,
            &binder,
            &line_map,
            "test.ts".to_string(),
            source,
        );

        // Searching for A should only find direct subclass B
        let pos = Position::new(0, 6);
        let result = provider.get_implementations(root, pos);

        assert!(result.is_some(), "Should find direct subclasses of A");
        let locs = result.unwrap();
        assert_eq!(
            locs.len(),
            1,
            "Should find only direct subclass (single-level)"
        );
        assert_eq!(locs[0].range.start.line, 1);
    }
}
