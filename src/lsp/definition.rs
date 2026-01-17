//! Go-to-Definition implementation for LSP.
//!
//! Given a position in the source, finds where the symbol at that position is defined.

use crate::lsp::position::{LineMap, Location, Position, Range};
use crate::lsp::resolver::{ScopeCache, ScopeCacheStats, ScopeWalker};
use crate::lsp::utils::find_node_at_offset;
use crate::parser::NodeIndex;
use crate::parser::thin_node::ThinNodeArena;
use crate::thin_binder::ThinBinderState;

/// Go-to-Definition provider.
///
/// This struct provides LSP "Go to Definition" functionality by:
/// 1. Converting a position to a byte offset
/// 2. Finding the AST node at that offset
/// 3. Resolving the node to a symbol
/// 4. Returning the symbol's declaration locations
pub struct GoToDefinition<'a> {
    arena: &'a ThinNodeArena,
    binder: &'a ThinBinderState,
    line_map: &'a LineMap,
    file_name: String,
    source_text: &'a str,
}

impl<'a> GoToDefinition<'a> {
    /// Create a new Go-to-Definition provider.
    pub fn new(
        arena: &'a ThinNodeArena,
        binder: &'a ThinBinderState,
        line_map: &'a LineMap,
        file_name: String,
        source_text: &'a str,
    ) -> Self {
        Self {
            arena,
            binder,
            line_map,
            file_name,
            source_text,
        }
    }

    /// Get the definition location(s) for the symbol at the given position.
    ///
    /// Returns a list of locations because a symbol can have multiple declarations
    /// (e.g., function overloads, merged declarations).
    ///
    /// Returns None if no symbol is found at the position.
    pub fn get_definition(&self, root: NodeIndex, position: Position) -> Option<Vec<Location>> {
        self.get_definition_internal(root, position, None, None)
    }

    pub fn get_definition_with_scope_cache(
        &self,
        root: NodeIndex,
        position: Position,
        scope_cache: &mut ScopeCache,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<Vec<Location>> {
        self.get_definition_internal(root, position, Some(scope_cache), scope_stats)
    }

    fn get_definition_internal(
        &self,
        root: NodeIndex,
        position: Position,
        scope_cache: Option<&mut ScopeCache>,
        mut scope_stats: Option<&mut ScopeCacheStats>,
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
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let symbol_id = if let Some(scope_cache) = scope_cache {
            walker.resolve_node_cached(root, node_idx, scope_cache, scope_stats.as_deref_mut())?
        } else {
            walker.resolve_node(root, node_idx)?
        };

        // 4. Get the symbol's declarations
        let symbol = self.binder.symbols.get(symbol_id)?;

        // 5. Convert declaration nodes to Locations
        let locations: Vec<Location> = symbol
            .declarations
            .iter()
            .filter_map(|&decl_idx| {
                let decl_node = self.arena.get(decl_idx)?;
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

    /// Get the definition location for a specific node (by NodeIndex).
    ///
    /// This is useful when you already have the node index from another operation.
    pub fn get_definition_for_node(
        &self,
        root: NodeIndex,
        node_idx: NodeIndex,
    ) -> Option<Vec<Location>> {
        self.get_definition_for_node_internal(root, node_idx, None, None)
    }

    pub fn get_definition_for_node_with_scope_cache(
        &self,
        root: NodeIndex,
        node_idx: NodeIndex,
        scope_cache: &mut ScopeCache,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<Vec<Location>> {
        self.get_definition_for_node_internal(root, node_idx, Some(scope_cache), scope_stats)
    }

    fn get_definition_for_node_internal(
        &self,
        root: NodeIndex,
        node_idx: NodeIndex,
        scope_cache: Option<&mut ScopeCache>,
        mut scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<Vec<Location>> {
        if node_idx.is_none() {
            return None;
        }

        // Resolve the node to a symbol
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let symbol_id = if let Some(scope_cache) = scope_cache {
            walker.resolve_node_cached(root, node_idx, scope_cache, scope_stats.as_deref_mut())?
        } else {
            walker.resolve_node(root, node_idx)?
        };

        // Get the symbol's declarations
        let symbol = self.binder.symbols.get(symbol_id)?;

        // Convert declaration nodes to Locations
        let locations: Vec<Location> = symbol
            .declarations
            .iter()
            .filter_map(|&decl_idx| {
                let decl_node = self.arena.get(decl_idx)?;
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
}

#[cfg(test)]
mod definition_tests {
    use super::*;
    use crate::lsp::position::LineMap;
    use crate::thin_binder::ThinBinderState;
    use crate::thin_parser::ThinParserState;

    #[test]
    fn test_goto_definition_simple_variable() {
        // const x = 1;
        // x + 1;
        let source = "const x = 1;\nx + 1;";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'x' in "x + 1" (line 1, column 0)
        let position = Position::new(1, 0);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        // Should find the definition at "const x = 1"
        assert!(definitions.is_some(), "Should find definition for x");

        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            // The definition should be on line 0
            assert_eq!(
                defs[0].range.start.line, 0,
                "Definition should be on line 0"
            );
        }
    }

    #[test]
    fn test_goto_definition_type_reference() {
        let source = "type Foo = { value: string };\nconst x: Foo = { value: \"\" };";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'Foo' in the type annotation (line 1)
        let position = Position::new(1, 9);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should find definition for type reference"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 0,
                "Definition should be on line 0"
            );
        }
    }

    #[test]
    fn test_goto_definition_binding_pattern() {
        let source = "const { foo } = obj;\nfoo;";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'foo' usage (line 1)
        let position = Position::new(1, 0);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should find definition for binding pattern name"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 0,
                "Definition should be on line 0"
            );
        }
    }

    #[test]
    fn test_goto_definition_parameter_binding_pattern() {
        let source = "function demo({ foo }: { foo: number }) {\n  return foo;\n}";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'foo' usage in the return (line 1)
        let position = Position::new(1, 9);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should find definition for parameter binding name"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 0,
                "Definition should be on line 0"
            );
        }
    }

    #[test]
    fn test_goto_definition_class_method_local() {
        let source = "class Foo {\n  method() {\n    const value = 1;\n    return value;\n  }\n}";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 3)
        let position = Position::new(3, 11);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should find definition for method local"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 2,
                "Definition should be on line 2"
            );
        }
    }

    #[test]
    fn test_goto_definition_class_method_name() {
        let source = "class Foo {\n  method() {}\n}";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'method' name (line 1)
        let position = Position::new(1, 2);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should find definition for method name"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 1,
                "Definition should be on line 1"
            );
        }
    }

    #[test]
    fn test_goto_definition_class_member_not_in_scope() {
        let source = "class Foo {\n  value = 1;\n  method() {\n    return value;\n  }\n}";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 3)
        let position = Position::new(3, 11);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_none(),
            "Class members should not resolve as lexical identifiers"
        );
    }

    #[test]
    fn test_goto_definition_class_self_reference() {
        let source = "class Foo {\n  method() {\n    return Foo;\n  }\n}";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'Foo' usage (line 2)
        let position = Position::new(2, 11);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should resolve class name within class scope"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 0,
                "Definition should be on line 0"
            );
        }
    }

    #[test]
    fn test_goto_definition_class_expression_name() {
        let source = "const Foo = class Bar {\n  method() {\n    return Bar;\n  }\n};";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'Bar' usage (line 2)
        let position = Position::new(2, 11);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should resolve class expression name in body"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 0,
                "Definition should be on line 0"
            );
        }
    }

    #[test]
    fn test_goto_definition_nested_arrow_in_conditional() {
        let source =
            "const handler = cond ? (() => {\n  const value = 1;\n  return value;\n}) : null;";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 2)
        let position = Position::new(2, 9);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(definitions.is_some(), "Should resolve nested arrow locals");
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 1,
                "Definition should be on line 1"
            );
        }
    }

    #[test]
    fn test_goto_definition_nested_arrow_in_if_condition() {
        let source = "if ((() => {\n  const value = 1;\n  return value;\n})()) {}";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 2)
        let position = Position::new(2, 9);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should resolve nested arrow locals in condition"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 1,
                "Definition should be on line 1"
            );
        }
    }

    #[test]
    fn test_goto_definition_nested_arrow_in_while_condition() {
        let source = "while ((() => {\n  const value = 1;\n  return value;\n})()) {}";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 2)
        let position = Position::new(2, 9);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should resolve nested arrow locals in while condition"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 1,
                "Definition should be on line 1"
            );
        }
    }

    #[test]
    fn test_goto_definition_nested_arrow_in_for_of_expression() {
        let source = "for (const item of (() => {\n  const value = 1;\n  return value;\n})()) {}";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 2)
        let position = Position::new(2, 9);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should resolve nested arrow locals in for-of expression"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 1,
                "Definition should be on line 1"
            );
        }
    }

    #[test]
    fn test_goto_definition_export_default_expression() {
        let source = "export default (() => {\n  const value = 1;\n  return value;\n})();";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 2)
        let position = Position::new(2, 9);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should resolve locals in export default expression"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 1,
                "Definition should be on line 1"
            );
        }
    }

    #[test]
    fn test_goto_definition_labeled_statement_local() {
        let source = "label: {\n  const value = 1;\n  value;\n}";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 2)
        let position = Position::new(2, 2);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should resolve locals inside labeled statement"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 1,
                "Definition should be on line 1"
            );
        }
    }

    #[test]
    fn test_goto_definition_with_statement_local() {
        let source = "with (obj) {\n  const value = 1;\n  value;\n}";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 2)
        let position = Position::new(2, 2);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should resolve locals inside with statement"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 1,
                "Definition should be on line 1"
            );
        }
    }

    #[test]
    fn test_goto_definition_var_hoisted_in_nested_block() {
        let source = "function demo() {\n  value;\n  if (cond) {\n    var value = 1;\n  }\n}";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage before the declaration (line 1)
        let position = Position::new(1, 2);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should resolve hoisted var definition"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 3,
                "Definition should be on line 3"
            );
        }
    }

    #[test]
    fn test_goto_definition_decorator_reference() {
        let source = "const deco = () => {};\n@deco\nclass Foo {}";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'deco' usage in the decorator (line 1)
        let position = Position::new(1, 1);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(definitions.is_some(), "Should resolve decorator reference");
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 0,
                "Definition should be on line 0"
            );
        }
    }

    #[test]
    fn test_goto_definition_decorator_argument_local() {
        let source = "const deco = (cb) => cb();\n@deco(() => {\n  const value = 1;\n  return value;\n})\nclass Foo {}";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage inside the decorator argument (line 3)
        let position = Position::new(3, 9);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should resolve locals inside decorator arguments"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 2,
                "Definition should be on line 2"
            );
        }
    }

    #[test]
    fn test_goto_definition_nested_arrow_in_object_literal() {
        let source = "const holder = { run: () => {\n  const value = 1;\n  return value;\n} };";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 2)
        let position = Position::new(2, 9);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should resolve nested object literal locals"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 1,
                "Definition should be on line 1"
            );
        }
    }

    #[test]
    fn test_goto_definition_class_static_block_local() {
        let source = "class Foo {\n  static {\n    const value = 1;\n    value;\n  }\n}";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 3)
        let position = Position::new(3, 4);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(definitions.is_some(), "Should resolve static block locals");
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 2,
                "Definition should be on line 2"
            );
        }
    }

    #[test]
    fn test_goto_definition_not_found() {
        let source = "const x = 1;";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position outside any identifier
        let position = Position::new(0, 11); // At the semicolon

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        // Should not find a definition
        assert!(
            definitions.is_none(),
            "Should not find definition at semicolon"
        );
    }
}
