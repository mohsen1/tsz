//! Find References implementation for LSP.
//!
//! Given a position in the source, finds all references to the symbol at that position.

use crate::binder::BinderState;
use crate::binder::SymbolId;
use crate::lsp::position::{LineMap, Location, Position, Range};
use crate::lsp::resolver::{ScopeCache, ScopeCacheStats, ScopeWalker};
use crate::lsp::utils::find_node_at_offset;
use crate::parser::node::NodeArena;
use crate::parser::{NodeIndex, syntax_kind_ext};
use crate::scanner::SyntaxKind;

/// Find References provider.
///
/// This struct provides LSP "Find References" functionality by:
/// 1. Converting a position to a byte offset
/// 2. Finding the AST node at that offset
/// 3. Resolving the node to a symbol
/// 4. Finding all references to that symbol in the AST
/// 5. Returning their locations
pub struct FindReferences<'a> {
    arena: &'a NodeArena,
    binder: &'a BinderState,
    line_map: &'a LineMap,
    file_name: String,
    source_text: &'a str,
}

impl<'a> FindReferences<'a> {
    /// Create a new Find References provider.
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

    /// Find all references to the symbol at the given position.
    ///
    /// Returns a list of locations where the symbol is referenced.
    /// This includes both the declaration(s) and all usages.
    ///
    /// Returns None if no symbol is found at the position.
    pub fn find_references(&self, root: NodeIndex, position: Position) -> Option<Vec<Location>> {
        self.find_references_internal(root, position, None, None)
    }

    pub fn find_references_with_scope_cache(
        &self,
        root: NodeIndex,
        position: Position,
        scope_cache: &mut ScopeCache,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<Vec<Location>> {
        self.find_references_internal(root, position, Some(scope_cache), scope_stats)
    }

    fn find_references_internal(
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

        // 3. Resolve the node to a symbol
        let symbol_id = self.resolve_symbol_internal(root, node_idx, scope_cache, scope_stats)?;

        // 4. Find all references to this symbol
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let ref_nodes = walker.find_references(root, symbol_id);

        // 5. Also include the declarations
        let symbol = self.binder.symbols.get(symbol_id)?;
        let mut all_nodes = ref_nodes.clone();
        all_nodes.extend(symbol.declarations.iter().copied());

        // Remove duplicates (a declaration might also be a reference)
        all_nodes.sort_by_key(|n| n.0);
        all_nodes.dedup();

        // 6. Convert to Locations
        let locations: Vec<Location> = all_nodes
            .iter()
            .filter_map(|&idx| self.location_for_node(idx))
            .collect();

        if locations.is_empty() {
            None
        } else {
            Some(locations)
        }
    }

    /// Find references for a specific node (by NodeIndex).
    ///
    /// This is useful when you already have the node index from another operation.
    pub fn find_references_for_node(
        &self,
        root: NodeIndex,
        node_idx: NodeIndex,
    ) -> Option<Vec<Location>> {
        self.find_references_for_node_internal(root, node_idx, None, None)
    }

    pub fn find_references_for_node_with_scope_cache(
        &self,
        root: NodeIndex,
        node_idx: NodeIndex,
        scope_cache: &mut ScopeCache,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<Vec<Location>> {
        self.find_references_for_node_internal(root, node_idx, Some(scope_cache), scope_stats)
    }

    pub fn find_references_for_symbol(
        &self,
        root: NodeIndex,
        symbol_id: SymbolId,
    ) -> Option<Vec<Location>> {
        if symbol_id.is_none() {
            return None;
        }

        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let ref_nodes = walker.find_references(root, symbol_id);

        let symbol = self.binder.symbols.get(symbol_id)?;
        let mut all_nodes = ref_nodes;
        all_nodes.extend(symbol.declarations.iter().copied());

        all_nodes.sort_by_key(|n| n.0);
        all_nodes.dedup();

        let locations: Vec<Location> = all_nodes
            .iter()
            .filter_map(|&idx| self.location_for_node(idx))
            .collect();

        if locations.is_empty() {
            None
        } else {
            Some(locations)
        }
    }

    fn find_references_for_node_internal(
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
        let symbol_id = self.resolve_symbol_internal(root, node_idx, scope_cache, scope_stats)?;

        // Find all references to this symbol
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let ref_nodes = walker.find_references(root, symbol_id);

        // Also include the declarations
        let symbol = self.binder.symbols.get(symbol_id)?;
        let mut all_nodes = ref_nodes.clone();
        all_nodes.extend(symbol.declarations.iter().copied());

        // Remove duplicates
        all_nodes.sort_by_key(|n| n.0);
        all_nodes.dedup();

        // Convert to Locations
        let locations: Vec<Location> = all_nodes
            .iter()
            .filter_map(|&idx| self.location_for_node(idx))
            .collect();

        if locations.is_empty() {
            None
        } else {
            Some(locations)
        }
    }

    /// Find only usages (excluding declarations) for the symbol at the given position.
    pub fn find_usages_only(&self, root: NodeIndex, position: Position) -> Option<Vec<Location>> {
        self.find_usages_only_internal(root, position, None, None)
    }

    pub fn find_usages_only_with_scope_cache(
        &self,
        root: NodeIndex,
        position: Position,
        scope_cache: &mut ScopeCache,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<Vec<Location>> {
        self.find_usages_only_internal(root, position, Some(scope_cache), scope_stats)
    }

    fn find_usages_only_internal(
        &self,
        root: NodeIndex,
        position: Position,
        scope_cache: Option<&mut ScopeCache>,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<Vec<Location>> {
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;
        let node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_none() {
            return None;
        }

        let symbol_id = self.resolve_symbol_internal(root, node_idx, scope_cache, scope_stats)?;

        // Find all references (usages only, not declarations)
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let ref_nodes = walker.find_references(root, symbol_id);

        // Convert to Locations
        let locations: Vec<Location> = ref_nodes
            .iter()
            .filter_map(|&idx| self.location_for_node(idx))
            .collect();

        if locations.is_empty() {
            None
        } else {
            Some(locations)
        }
    }

    fn location_for_node(&self, idx: NodeIndex) -> Option<Location> {
        let target_idx = self.name_node_for(idx).unwrap_or(idx);
        let node = self.arena.get(target_idx)?;
        let start_pos = self.line_map.offset_to_position(node.pos, self.source_text);
        let end_pos = self.line_map.offset_to_position(node.end, self.source_text);

        Some(Location {
            file_path: self.file_name.clone(),
            range: Range::new(start_pos, end_pos),
        })
    }

    fn name_node_for(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.arena.get(idx)?;
        match node.kind {
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                let decl = self.arena.get_variable_declaration(node)?;
                if decl.name.is_none() {
                    None
                } else {
                    Some(decl.name)
                }
            }
            k if k == syntax_kind_ext::PARAMETER => {
                let param = self.arena.get_parameter(node)?;
                if param.name.is_none() {
                    None
                } else {
                    Some(param.name)
                }
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                let func = self.arena.get_function(node)?;
                if func.name.is_none() {
                    None
                } else {
                    Some(func.name)
                }
            }
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
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                let alias = self.arena.get_type_alias(node)?;
                if alias.name.is_none() {
                    None
                } else {
                    Some(alias.name)
                }
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                let enm = self.arena.get_enum(node)?;
                if enm.name.is_none() {
                    None
                } else {
                    Some(enm.name)
                }
            }
            k if k == syntax_kind_ext::ENUM_MEMBER => {
                let member = self.arena.get_enum_member(node)?;
                if member.name.is_none() {
                    None
                } else {
                    Some(member.name)
                }
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                let module = self.arena.get_module(node)?;
                if module.name.is_none() {
                    None
                } else {
                    Some(module.name)
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let method = self.arena.get_method_decl(node)?;
                if method.name.is_none() {
                    None
                } else {
                    Some(method.name)
                }
            }
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                let prop = self.arena.get_property_decl(node)?;
                if prop.name.is_none() {
                    None
                } else {
                    Some(prop.name)
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                let accessor = self.arena.get_accessor(node)?;
                if accessor.name.is_none() {
                    None
                } else {
                    Some(accessor.name)
                }
            }
            k if k == syntax_kind_ext::IMPORT_SPECIFIER => {
                let spec = self.arena.get_specifier(node)?;
                if !spec.name.is_none() {
                    Some(spec.name)
                } else if !spec.property_name.is_none() {
                    Some(spec.property_name)
                } else {
                    None
                }
            }
            k if k == syntax_kind_ext::EXPORT_SPECIFIER => {
                let spec = self.arena.get_specifier(node)?;
                if !spec.property_name.is_none() {
                    Some(spec.property_name)
                } else if !spec.name.is_none() {
                    Some(spec.name)
                } else {
                    None
                }
            }
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                let import = self.arena.get_import_decl(node)?;
                if import.import_clause.is_none() {
                    return None;
                }
                let clause_node = self.arena.get(import.import_clause)?;
                if clause_node.kind == SyntaxKind::Identifier as u16 {
                    Some(import.import_clause)
                } else {
                    None
                }
            }
            k if k == syntax_kind_ext::TYPE_PARAMETER => {
                let param = self.arena.get_type_parameter(node)?;
                if param.name.is_none() {
                    None
                } else {
                    Some(param.name)
                }
            }
            _ => None,
        }
    }

    pub(crate) fn resolve_symbol_for_node_with_scope_cache(
        &self,
        root: NodeIndex,
        node_idx: NodeIndex,
        scope_cache: &mut ScopeCache,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<SymbolId> {
        self.resolve_symbol_internal(root, node_idx, Some(scope_cache), scope_stats)
    }

    fn resolve_symbol_internal(
        &self,
        root: NodeIndex,
        node_idx: NodeIndex,
        mut scope_cache: Option<&mut ScopeCache>,
        mut scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<SymbolId> {
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let symbol_id = if let Some(scope_cache) = scope_cache.as_deref_mut() {
            walker.resolve_node_cached(root, node_idx, scope_cache, scope_stats.as_deref_mut())
        } else {
            walker.resolve_node(root, node_idx)
        };

        if symbol_id.is_some() {
            return symbol_id;
        }

        let tag_idx = self.tagged_template_tag(node_idx)?;
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        if let Some(scope_cache) = scope_cache {
            walker.resolve_node_cached(root, tag_idx, scope_cache, scope_stats)
        } else {
            walker.resolve_node(root, tag_idx)
        }
    }

    fn tagged_template_tag(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.arena.get(node_idx)?;
        let is_template_node = matches!(
            node.kind,
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION
                || k == syntax_kind_ext::TEMPLATE_SPAN
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == SyntaxKind::TemplateHead as u16
                || k == SyntaxKind::TemplateMiddle as u16
                || k == SyntaxKind::TemplateTail as u16
        );

        if !is_template_node {
            return None;
        }

        let mut current = node_idx;
        while let Some(ext) = self.arena.get_extended(current) {
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }
            let parent_node = self.arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION {
                return self
                    .arena
                    .tagged_templates
                    .get(parent_node.data_index as usize)
                    .map(|tagged| tagged.tag);
            }
            current = parent;
        }

        None
    }
}

#[cfg(test)]
mod references_tests {
    use super::*;
    use crate::binder::BinderState;
    use crate::lsp::position::LineMap;
    use crate::parser::ParserState;

    #[test]
    fn test_find_references_simple() {
        // const x = 1;
        // x + x;
        let source = "const x = 1;\nx + x;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the first 'x' in "x + x" (line 1, column 0)
        let position = Position::new(1, 0);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(references.is_some(), "Should find references for x");

        if let Some(refs) = references {
            // Should find at least the declaration and two usages
            assert!(
                refs.len() >= 2,
                "Should find at least 2 references (declaration + usages)"
            );
        }
    }

    #[test]
    fn test_find_references_for_symbol() {
        let source = "const x = 1;\nx + x;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let symbol_id = binder.file_locals.get("x").expect("Expected symbol for x");

        let line_map = LineMap::build(source);
        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references_for_symbol(root, symbol_id);

        assert!(references.is_some(), "Should find references for x");
        if let Some(refs) = references {
            assert!(
                refs.len() >= 2,
                "Should find at least 2 references (declaration + usages)"
            );
        }
    }

    #[test]
    fn test_find_references_not_found() {
        let source = "const x = 1;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position outside any identifier
        let position = Position::new(0, 11); // At the semicolon

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        // Should not find references
        assert!(
            references.is_none(),
            "Should not find references at semicolon"
        );
    }

    #[test]
    fn test_find_references_template_expression() {
        let source = "const name = \"Ada\";\nconst msg = `hi ${name}`;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'name' inside the template expression (line 1)
        let position = Position::new(1, 18);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references in template expression"
        );
        let refs = references.unwrap();
        assert!(
            refs.len() >= 2,
            "Should find declaration and template usage"
        );
    }

    #[test]
    fn test_find_references_jsx_expression() {
        let source = "const name = \"Ada\";\nconst el = <div>{name}</div>;";
        let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'name' inside JSX expression (line 1)
        let position = Position::new(1, 17);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.tsx".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references in JSX expression"
        );
        let refs = references.unwrap();
        assert!(refs.len() >= 2, "Should find declaration and JSX usage");
    }

    #[test]
    fn test_find_references_await_expression() {
        let source = "const value = 1;\nasync function run() {\n  await value;\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' inside await (line 2)
        let position = Position::new(2, 8);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references in await expression"
        );
        let refs = references.unwrap();
        assert!(refs.len() >= 2, "Should find declaration and await usage");
    }

    #[test]
    fn test_find_references_tagged_template_expression() {
        let source =
            "const tag = (strings: TemplateStringsArray) => strings[0];\nconst msg = tag`hello`;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'tag' inside tagged template (line 1)
        let position = Position::new(1, 16);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references in tagged template"
        );
        let refs = references.unwrap();
        assert!(
            refs.len() >= 2,
            "Should find declaration and tagged template usage"
        );
    }

    #[test]
    fn test_find_references_as_expression() {
        let source = "const value = 1;\nconst result = value as number;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' inside the as-expression (line 1)
        let position = Position::new(1, 15);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references in as expression"
        );
        let refs = references.unwrap();
        assert!(
            refs.len() >= 2,
            "Should find declaration and as-expression usage"
        );
    }

    #[test]
    fn test_find_references_binding_pattern() {
        let source = "const { foo } = obj;\nfoo;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'foo' usage (line 1)
        let position = Position::new(1, 0);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references for binding pattern name"
        );
        let refs = references.unwrap();
        assert!(refs.len() >= 2, "Should find declaration and usage");
    }

    #[test]
    fn test_find_references_binding_pattern_initializer() {
        let source = "const value = 1;\nconst { foo = value } = obj;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' inside the initializer (line 1)
        let position = Position::new(1, 14);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references in binding pattern initializer"
        );
        let refs = references.unwrap();
        assert!(
            refs.len() >= 2,
            "Should find declaration and initializer usage"
        );
    }

    #[test]
    fn test_find_references_parameter_binding_pattern() {
        let source = "function demo({ foo }: { foo: number }) {\n  return foo;\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'foo' usage in the return (line 1)
        let position = Position::new(1, 9);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references for parameter binding name"
        );
        let refs = references.unwrap();
        assert!(
            refs.len() >= 2,
            "Should find parameter declaration and usage"
        );
    }

    #[test]
    fn test_find_references_parameter_array_binding() {
        let source = "function demo([foo]: number[]) {\n  return foo;\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'foo' usage in the return (line 1)
        let position = Position::new(1, 9);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references for array binding name"
        );
        let refs = references.unwrap();
        assert!(
            refs.len() >= 2,
            "Should find parameter declaration and usage"
        );
    }

    #[test]
    fn test_find_references_nested_arrow_in_switch_case() {
        let source = "switch (state) {\n  case (() => {\n    const value = 1;\n    return value;\n  })():\n    break;\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 3)
        let position = Position::new(3, 11);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references for switch case locals"
        );
        let refs = references.unwrap();
        assert!(refs.len() >= 2, "Should find declaration and usage");
    }

    #[test]
    fn test_find_references_nested_arrow_in_if_condition() {
        let source = "if ((() => {\n  const value = 1;\n  return value;\n})()) {}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 2)
        let position = Position::new(2, 9);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references for nested arrow locals in condition"
        );
        let refs = references.unwrap();
        assert!(refs.len() >= 2, "Should find declaration and usage");
    }

    #[test]
    fn test_find_references_export_default_expression() {
        let source = "export default (() => {\n  const value = 1;\n  return value;\n})();";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 2)
        let position = Position::new(2, 9);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references for export default expression locals"
        );
        let refs = references.unwrap();
        assert!(refs.len() >= 2, "Should find declaration and usage");
    }

    #[test]
    fn test_find_references_labeled_statement_local() {
        let source = "label: {\n  const value = 1;\n  value;\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 2)
        let position = Position::new(2, 2);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references for labeled statement locals"
        );
        let refs = references.unwrap();
        assert!(refs.len() >= 2, "Should find declaration and usage");
    }

    #[test]
    fn test_find_references_with_statement_local() {
        let source = "with (obj) {\n  const value = 1;\n  value;\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 2)
        let position = Position::new(2, 2);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references for with statement locals"
        );
        let refs = references.unwrap();
        assert!(refs.len() >= 2, "Should find declaration and usage");
    }

    #[test]
    fn test_find_references_var_hoisted_in_nested_block() {
        let source = "function demo() {\n  value;\n  if (cond) {\n    var value = 1;\n  }\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage before the declaration (line 1)
        let position = Position::new(1, 2);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references for hoisted var"
        );
        let refs = references.unwrap();
        assert!(refs.len() >= 2, "Should find declaration and usage");
    }

    #[test]
    fn test_find_references_decorator_reference() {
        let source = "const deco = () => {};\n@deco\nclass Foo {}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'deco' usage in the decorator (line 1)
        let position = Position::new(1, 1);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references for decorator usage"
        );
        let refs = references.unwrap();
        assert!(refs.len() >= 2, "Should find declaration and usage");
    }

    #[test]
    fn test_find_references_class_method_local() {
        let source = "class Foo {\n  method() {\n    const value = 1;\n    return value;\n  }\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 3)
        let position = Position::new(3, 11);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references for method local"
        );
        let refs = references.unwrap();
        assert!(refs.len() >= 2, "Should find declaration and usage");
    }

    #[test]
    fn test_find_references_class_self_reference() {
        let source = "class Foo {\n  method() {\n    return Foo;\n  }\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'Foo' usage inside the method (line 2)
        let position = Position::new(2, 11);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references for class self name"
        );
        let refs = references.unwrap();
        assert!(refs.len() >= 2, "Should find declaration and usage");
    }

    #[test]
    fn test_find_references_class_expression_name() {
        let source = "const Foo = class Bar {\n  method() {\n    return Bar;\n  }\n};";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'Bar' usage inside the method (line 2)
        let position = Position::new(2, 11);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references for class expression name"
        );
        let refs = references.unwrap();
        assert!(refs.len() >= 2, "Should find declaration and usage");
    }

    #[test]
    fn test_find_references_class_static_block_local() {
        let source = "class Foo {\n  static {\n    const value = 1;\n    value;\n  }\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage inside the static block (line 3)
        let position = Position::new(3, 4);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references for static block locals"
        );
        let refs = references.unwrap();
        assert!(refs.len() >= 2, "Should find declaration and usage");
    }
}
