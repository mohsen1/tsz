//! Go-to-Definition implementation for LSP.
//!
//! Given a position in the source, finds where the symbol at that position is defined.

use crate::binder::{BinderState, SymbolId};
use crate::lsp::position::{LineMap, Location, Position, Range};
use crate::lsp::resolver::{ScopeCache, ScopeCacheStats, ScopeWalker};
use crate::lsp::utils::find_node_at_offset;
use crate::parser::NodeIndex;
use crate::parser::node::NodeArena;

/// Well-known built-in global identifiers that are provided by the runtime
/// environment and not defined in user source files.
/// When these are encountered and no declaration is found, we return None
/// instead of crashing or returning garbage positions.
const BUILTIN_GLOBALS: &[&str] = &[
    // Console API
    "console",
    // Fundamental objects
    "Object",
    "Function",
    "Boolean",
    "Symbol",
    // Error types
    "Error",
    "AggregateError",
    "EvalError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "TypeError",
    "URIError",
    // Numbers and dates
    "Number",
    "BigInt",
    "Math",
    "Date",
    "Infinity",
    "NaN",
    "undefined",
    // Text processing
    "String",
    "RegExp",
    // Indexed collections
    "Array",
    "Int8Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "Int16Array",
    "Uint16Array",
    "Int32Array",
    "Uint32Array",
    "Float32Array",
    "Float64Array",
    "BigInt64Array",
    "BigUint64Array",
    // Keyed collections
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
    "WeakRef",
    // Structured data
    "ArrayBuffer",
    "SharedArrayBuffer",
    "Atomics",
    "DataView",
    "JSON",
    // Control abstraction
    "Promise",
    "Generator",
    "GeneratorFunction",
    "AsyncFunction",
    "AsyncGenerator",
    "AsyncGeneratorFunction",
    // Reflection
    "Reflect",
    "Proxy",
    // Internationalization
    "Intl",
    // Web APIs
    "globalThis",
    "window",
    "document",
    "navigator",
    "location",
    "history",
    "localStorage",
    "sessionStorage",
    "fetch",
    "Headers",
    "Request",
    "Response",
    "URL",
    "URLSearchParams",
    "setTimeout",
    "setInterval",
    "clearTimeout",
    "clearInterval",
    "requestAnimationFrame",
    "cancelAnimationFrame",
    "queueMicrotask",
    "structuredClone",
    "atob",
    "btoa",
    "TextEncoder",
    "TextDecoder",
    "AbortController",
    "AbortSignal",
    "Blob",
    "File",
    "FileReader",
    "FormData",
    "ReadableStream",
    "WritableStream",
    "TransformStream",
    "Event",
    "EventTarget",
    "CustomEvent",
    "MutationObserver",
    "IntersectionObserver",
    "ResizeObserver",
    "PerformanceObserver",
    "WebSocket",
    "Worker",
    "MessageChannel",
    "MessagePort",
    "BroadcastChannel",
    // Node.js globals
    "process",
    "Buffer",
    "require",
    "module",
    "exports",
    "__dirname",
    "__filename",
    "global",
    // TypeScript utility types (may appear as identifiers)
    "Partial",
    "Required",
    "Readonly",
    "Record",
    "Pick",
    "Omit",
    "Exclude",
    "Extract",
    "NonNullable",
    "Parameters",
    "ConstructorParameters",
    "ReturnType",
    "InstanceType",
    "ThisParameterType",
    "OmitThisParameter",
    "ThisType",
    "Awaited",
    // Iterator/Iterable
    "Iterator",
    "IterableIterator",
    "AsyncIterableIterator",
];

/// Check if a name is a well-known built-in global.
fn is_builtin_global(name: &str) -> bool {
    BUILTIN_GLOBALS.contains(&name)
}

/// Go-to-Definition provider.
///
/// This struct provides LSP "Go to Definition" functionality by:
/// 1. Converting a position to a byte offset
/// 2. Finding the AST node at that offset
/// 3. Resolving the node to a symbol
/// 4. Returning the symbol's declaration locations
pub struct GoToDefinition<'a> {
    arena: &'a NodeArena,
    binder: &'a BinderState,
    line_map: &'a LineMap,
    file_name: String,
    source_text: &'a str,
}

impl<'a> GoToDefinition<'a> {
    /// Create a new Go-to-Definition provider.
    pub fn new(
        arena: &'a NodeArena,
        binder: &'a BinderState,
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
        scope_stats: Option<&mut ScopeCacheStats>,
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

        // 3. Resolve the node to a symbol via scope walking
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let symbol_id_opt = if let Some(scope_cache) = scope_cache {
            walker.resolve_node_cached(root, node_idx, scope_cache, scope_stats)
        } else {
            walker.resolve_node(root, node_idx)
        };

        // 4. If primary resolution succeeded, use the symbol
        if let Some(symbol_id) = symbol_id_opt {
            if let Some(locations) = self.locations_from_symbol(symbol_id) {
                return Some(locations);
            }
        }

        // 5. Fallback: try file_locals lookup by identifier text
        if let Some(locations) = self.try_file_locals_fallback(node_idx) {
            return Some(locations);
        }

        None
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
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<Vec<Location>> {
        if node_idx.is_none() {
            return None;
        }

        // Resolve the node to a symbol
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let symbol_id_opt = if let Some(scope_cache) = scope_cache {
            walker.resolve_node_cached(root, node_idx, scope_cache, scope_stats)
        } else {
            walker.resolve_node(root, node_idx)
        };

        // If primary resolution succeeded, use the symbol
        if let Some(symbol_id) = symbol_id_opt {
            if let Some(locations) = self.locations_from_symbol(symbol_id) {
                return Some(locations);
            }
        }

        // Fallback: try file_locals
        if let Some(locations) = self.try_file_locals_fallback(node_idx) {
            return Some(locations);
        }

        None
    }

    /// Convert a symbol's declarations into validated Location objects.
    ///
    /// This validates that declaration positions are within the source text bounds
    /// to prevent crashes when declarations point to other files or invalid positions.
    fn locations_from_symbol(&self, symbol_id: SymbolId) -> Option<Vec<Location>> {
        let symbol = self.binder.symbols.get(symbol_id)?;
        let source_len = self.source_text.len() as u32;

        let locations: Vec<Location> = symbol
            .declarations
            .iter()
            .filter_map(|&decl_idx| {
                let decl_node = self.arena.get(decl_idx)?;

                // Validate that positions are within the current file's bounds.
                // Declarations from other files (cross-file references, built-ins)
                // will have node indices that either don't exist in this arena or
                // have positions outside this file's text range.
                if decl_node.pos > source_len || decl_node.end > source_len {
                    return None;
                }
                if decl_node.end < decl_node.pos {
                    return None;
                }

                let start_pos = self
                    .line_map
                    .offset_to_position(decl_node.pos, self.source_text);
                let end_pos = self
                    .line_map
                    .offset_to_position(decl_node.end, self.source_text);

                // Validate computed positions are within the line map bounds
                let line_count = self.line_map.line_count() as u32;
                if start_pos.line >= line_count || end_pos.line >= line_count {
                    return None;
                }

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

    /// Try to resolve a node's identifier text via the binder's file_locals table.
    ///
    /// This serves as a fallback when the scope-based resolution fails (e.g., for
    /// shorthand properties, certain export patterns, etc.)
    fn try_file_locals_fallback(&self, node_idx: NodeIndex) -> Option<Vec<Location>> {
        let node = self.arena.get(node_idx)?;
        let pos = node.pos as usize;
        let end = node.end as usize;
        if end > self.source_text.len() || pos > end {
            return None;
        }

        let text = &self.source_text[pos..end];

        // Skip if this is a built-in global - no definition in user source
        if is_builtin_global(text) {
            return None;
        }

        // Try looking up in file_locals
        let symbol_id = self.binder.file_locals.get(text)?;
        self.locations_from_symbol(symbol_id)
    }
}

#[cfg(test)]
mod definition_tests {
    use super::*;
    use crate::binder::BinderState;
    use crate::lsp::position::LineMap;
    use crate::parser::ParserState;

    #[test]
    fn test_goto_definition_simple_variable() {
        // const x = 1;
        // x + 1;
        let source = "const x = 1;\nx + 1;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
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
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
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
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
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
    #[ignore] // TODO: Fix this test
    fn test_goto_definition_parameter_binding_pattern() {
        let source = "function demo({ foo }: { foo: number }) {\n  return foo;\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
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
    #[ignore] // TODO: Fix this test
    fn test_goto_definition_class_method_local() {
        let source = "class Foo {\n  method() {\n    const value = 1;\n    return value;\n  }\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
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
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
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
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
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
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
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
    #[ignore] // TODO: Fix this test
    fn test_goto_definition_class_expression_name() {
        let source = "const Foo = class Bar {\n  method() {\n    return Bar;\n  }\n};";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
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
    #[ignore] // TODO: Fix this test
    fn test_goto_definition_nested_arrow_in_conditional() {
        let source =
            "const handler = cond ? (() => {\n  const value = 1;\n  return value;\n}) : null;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
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
    #[ignore] // TODO: Fix this test
    fn test_goto_definition_nested_arrow_in_if_condition() {
        let source = "if ((() => {\n  const value = 1;\n  return value;\n})()) {}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
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
    #[ignore] // TODO: Fix this test
    fn test_goto_definition_nested_arrow_in_while_condition() {
        let source = "while ((() => {\n  const value = 1;\n  return value;\n})()) {}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
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
    #[ignore] // TODO: Fix this test
    fn test_goto_definition_nested_arrow_in_for_of_expression() {
        let source = "for (const item of (() => {\n  const value = 1;\n  return value;\n})()) {}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
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
    #[ignore] // TODO: Fix this test
    fn test_goto_definition_export_default_expression() {
        let source = "export default (() => {\n  const value = 1;\n  return value;\n})();";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
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
    #[ignore] // TODO: Fix this test
    fn test_goto_definition_labeled_statement_local() {
        let source = "label: {\n  const value = 1;\n  value;\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
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
    #[ignore] // TODO: Fix this test
    fn test_goto_definition_with_statement_local() {
        let source = "with (obj) {\n  const value = 1;\n  value;\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
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
    #[ignore] // TODO: Fix this test
    fn test_goto_definition_var_hoisted_in_nested_block() {
        let source = "function demo() {\n  value;\n  if (cond) {\n    var value = 1;\n  }\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
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
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
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
    #[ignore] // TODO: Fix this test
    fn test_goto_definition_decorator_argument_local() {
        let source = "const deco = (cb) => cb();\n@deco(() => {\n  const value = 1;\n  return value;\n})\nclass Foo {}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
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
    #[ignore] // TODO: Fix this test
    fn test_goto_definition_nested_arrow_in_object_literal() {
        let source = "const holder = { run: () => {\n  const value = 1;\n  return value;\n} };";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
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
    #[ignore] // TODO: Fix this test
    fn test_goto_definition_class_static_block_local() {
        let source = "class Foo {\n  static {\n    const value = 1;\n    value;\n  }\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
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
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
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

    // =========================================================================
    // New edge case tests
    // =========================================================================

    #[test]
    fn test_goto_definition_builtin_console_returns_none() {
        // "console" is a built-in global with no user declaration.
        // Should return None gracefully instead of crashing.
        let source = "console.log('hello');";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at "console" (line 0, column 0)
        let position = Position::new(0, 0);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        // Should return None (no crash) since console is a built-in
        assert!(
            definitions.is_none(),
            "Built-in global 'console' should return None, not crash"
        );
    }

    #[test]
    fn test_goto_definition_builtin_array_returns_none() {
        let source = "const arr = new Array(10);";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at "Array" (line 0, column 16)
        let position = Position::new(0, 16);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_none(),
            "Built-in global 'Array' should return None"
        );
    }

    #[test]
    fn test_goto_definition_builtin_promise_returns_none() {
        let source = "const p: Promise<number> = Promise.resolve(42);";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the Promise usage (after the =)
        let position = Position::new(0, 27);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_none(),
            "Built-in global 'Promise' should return None"
        );
    }

    #[test]
    fn test_goto_definition_no_crash_on_position_beyond_file() {
        let source = "const x = 1;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position way beyond the file (line 100, column 0)
        let position = Position::new(100, 0);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        // Should return None (no crash)
        assert!(
            definitions.is_none(),
            "Position beyond file should return None without crash"
        );
    }

    #[test]
    fn test_goto_definition_empty_source() {
        let source = "";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        let position = Position::new(0, 0);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_none(),
            "Empty source should return None without crash"
        );
    }

    #[test]
    fn test_goto_definition_self_declaration_identifier() {
        // Clicking on the declaration itself should navigate to it
        let source = "function hello() {}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at "hello" in the function declaration (line 0, column 9)
        let position = Position::new(0, 9);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        // Should find the declaration (itself)
        assert!(
            definitions.is_some(),
            "Should find declaration for function name"
        );
        if let Some(defs) = definitions {
            assert_eq!(defs[0].range.start.line, 0);
        }
    }

    #[test]
    fn test_goto_definition_is_builtin_global_helper() {
        // Test the is_builtin_global helper function directly
        assert!(is_builtin_global("console"));
        assert!(is_builtin_global("Array"));
        assert!(is_builtin_global("Promise"));
        assert!(is_builtin_global("Map"));
        assert!(is_builtin_global("Set"));
        assert!(is_builtin_global("setTimeout"));
        assert!(is_builtin_global("fetch"));
        assert!(is_builtin_global("process"));
        assert!(is_builtin_global("Buffer"));

        // User-defined names should NOT be built-in
        assert!(!is_builtin_global("myFunction"));
        assert!(!is_builtin_global("MyClass"));
        assert!(!is_builtin_global("handler"));
        assert!(!is_builtin_global("data"));
    }

    #[test]
    fn test_goto_definition_multiple_builtin_globals_no_crash() {
        // Multiple built-in references in one file should all return None
        let source =
            "console.log(Array.from([1, 2, 3]));\nPromise.resolve(42);\nsetTimeout(() => {}, 100);";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        // console at (0, 0)
        let d1 = goto_def.get_definition(root, Position::new(0, 0));
        assert!(d1.is_none(), "console should return None");

        // Promise at (1, 0)
        let d2 = goto_def.get_definition(root, Position::new(1, 0));
        assert!(d2.is_none(), "Promise should return None");

        // setTimeout at (2, 0)
        let d3 = goto_def.get_definition(root, Position::new(2, 0));
        assert!(d3.is_none(), "setTimeout should return None");
    }

    #[test]
    fn test_goto_definition_interface_reference() {
        // Interface declarations should be findable
        let source = "interface IFoo { bar: string; }\nconst x: IFoo = { bar: 'hi' };";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at "IFoo" type reference on line 1
        let position = Position::new(1, 9);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        // We expect this to either find the interface or return None gracefully
        // (no crash is the critical requirement)
        if let Some(defs) = &definitions {
            assert_eq!(
                defs[0].range.start.line, 0,
                "Interface definition should be on line 0"
            );
        }
    }

    #[test]
    fn test_goto_definition_enum_reference() {
        let source = "enum Color { Red, Green, Blue }\nconst c: Color = Color.Red;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at "Color" value reference on line 1 (after the =)
        let position = Position::new(1, 17);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        // No crash is the critical requirement
        if let Some(defs) = &definitions {
            assert_eq!(
                defs[0].range.start.line, 0,
                "Enum definition should be on line 0"
            );
        }
    }

    #[test]
    fn test_goto_definition_default_export_function() {
        // Export default function should be navigable
        let source = "export default function greet() { return 'hi'; }";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at "greet" (line 0, column 24)
        let position = Position::new(0, 24);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        // Should find the function declaration or not crash
        if let Some(defs) = &definitions {
            assert_eq!(defs[0].range.start.line, 0);
        }
    }

    #[test]
    fn test_goto_definition_validated_positions_are_in_bounds() {
        // Ensure returned positions are always within the source text bounds
        let source = "const x = 1;\nconst y = x + 2;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Try every possible valid position in the source
        let line_count = line_map.line_count() as u32;
        for line in 0..line_count {
            for col in 0..50 {
                let position = Position::new(line, col);
                let goto_def =
                    GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
                let definitions = goto_def.get_definition(root, position);

                // If we got definitions, all positions must be in bounds
                if let Some(defs) = definitions {
                    for def in &defs {
                        assert!(
                            def.range.start.line < line_count,
                            "Start line {} should be < line_count {}",
                            def.range.start.line,
                            line_count
                        );
                        assert!(
                            def.range.end.line < line_count,
                            "End line {} should be < line_count {}",
                            def.range.end.line,
                            line_count
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_goto_definition_for_node_with_none_index() {
        let source = "const x = 1;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition_for_node(root, NodeIndex::NONE);

        assert!(
            definitions.is_none(),
            "Should return None for NodeIndex::none()"
        );
    }
}
