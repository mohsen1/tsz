//! Hover implementation for LSP.
//!
//! Displays type information and documentation for the symbol at the cursor.

use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::lsp::jsdoc::{jsdoc_for_node, parse_jsdoc};
use crate::lsp::position::{LineMap, Position, Range};
use crate::lsp::resolver::{ScopeCache, ScopeCacheStats, ScopeWalker};
use crate::lsp::utils::find_node_at_or_before_offset;
use crate::parser::NodeIndex;
use crate::parser::node::NodeArena;
use crate::solver::TypeInterner;

/// Information returned for a hover request.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HoverInfo {
    /// The contents of the hover (usually Markdown)
    pub contents: Vec<String>,
    /// The range of the symbol being hovered
    pub range: Option<Range>,
}

/// Hover provider.
pub struct HoverProvider<'a> {
    arena: &'a NodeArena,
    binder: &'a BinderState,
    line_map: &'a LineMap,
    interner: &'a TypeInterner,
    source_text: &'a str,
    file_name: String,
    strict: bool,
}

impl<'a> HoverProvider<'a> {
    /// Create a new Hover provider.
    pub fn new(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        line_map: &'a LineMap,
        interner: &'a TypeInterner,
        source_text: &'a str,
        file_name: String,
    ) -> Self {
        Self {
            arena,
            binder,
            line_map,
            interner,
            source_text,
            file_name,
            strict: false,
        }
    }

    /// Create a new Hover provider with explicit strict mode setting.
    pub fn with_strict(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        line_map: &'a LineMap,
        interner: &'a TypeInterner,
        source_text: &'a str,
        file_name: String,
        strict: bool,
    ) -> Self {
        Self {
            arena,
            binder,
            line_map,
            interner,
            source_text,
            file_name,
            strict,
        }
    }

    /// Get hover information at the given position.
    ///
    /// # Arguments
    /// * `root` - The root node of the AST
    /// * `position` - The cursor position
    /// * `type_cache` - Mutable reference to the persistent type cache (for performance)
    pub fn get_hover(
        &self,
        root: NodeIndex,
        position: Position,
        type_cache: &mut Option<crate::checker::TypeCache>,
    ) -> Option<HoverInfo> {
        self.get_hover_internal(root, position, type_cache, None, None)
    }

    pub fn get_hover_with_scope_cache(
        &self,
        root: NodeIndex,
        position: Position,
        type_cache: &mut Option<crate::checker::TypeCache>,
        scope_cache: &mut ScopeCache,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<HoverInfo> {
        self.get_hover_internal(root, position, type_cache, Some(scope_cache), scope_stats)
    }

    fn get_hover_internal(
        &self,
        root: NodeIndex,
        position: Position,
        type_cache: &mut Option<crate::checker::TypeCache>,
        scope_cache: Option<&mut ScopeCache>,
        mut scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<HoverInfo> {
        // 1. Find node at position
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;
        let node_idx = find_node_at_or_before_offset(self.arena, offset, self.source_text);

        if node_idx.is_none() {
            return None;
        }

        // 2. Resolve symbol using ScopeWalker
        // We use ScopeWalker to handle local scopes correctly
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let symbol_id = if let Some(scope_cache) = scope_cache {
            walker.resolve_node_cached(root, node_idx, scope_cache, scope_stats.as_deref_mut())?
        } else {
            walker.resolve_node(root, node_idx)?
        };
        let symbol = self.binder.symbols.get(symbol_id)?;

        // 3. Compute Type Information
        // Use persistent cache if available for O(1) lookups on repeated queries
        let compiler_options = crate::checker::context::CheckerOptions {
            strict: self.strict,
            no_implicit_any: self.strict,
            no_implicit_returns: false,
            no_implicit_this: self.strict,
            strict_null_checks: self.strict,
            strict_function_types: self.strict,
            strict_property_initialization: self.strict,
            use_unknown_in_catch_variables: self.strict,
            isolated_modules: false,
            ..Default::default()
        };
        let mut checker = if let Some(cache) = type_cache.take() {
            CheckerState::with_cache(
                self.arena,
                self.binder,
                self.interner,
                self.file_name.clone(),
                cache,
                compiler_options,
            )
        } else {
            CheckerState::new(
                self.arena,
                self.binder,
                self.interner,
                self.file_name.clone(),
                compiler_options,
            )
        };

        let type_id = checker.get_type_of_symbol(symbol_id);
        let type_string = checker.format_type(type_id);

        // Extract and save the updated cache for future queries
        *type_cache = Some(checker.extract_cache());

        // 4. Construct the signature string
        // e.g. "(variable) x: number" or "(function) foo(): void"
        let kind_str = self.get_symbol_kind_string(symbol);
        let declaration_str = format!("({}) {}: {}", kind_str, symbol.escaped_name, type_string);

        // 5. Extract Documentation (JSDoc)
        // Look at the declaration node (value_declaration or first declaration)
        let decl_node_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else if let Some(&first) = symbol.declarations.first() {
            first
        } else {
            NodeIndex::NONE
        };

        let documentation = if !decl_node_idx.is_none() {
            jsdoc_for_node(self.arena, root, decl_node_idx, self.source_text)
        } else {
            String::new()
        };
        let documentation = self.format_jsdoc_for_hover(&documentation);

        // 6. Build response
        let mut contents = Vec::new();

        // Code block for the signature
        contents.push(format!("```typescript\n{}\n```", declaration_str));

        // Documentation paragraph
        if let Some(documentation) = documentation {
            contents.push(documentation);
        }

        // Calculate range for the hovered identifier
        let node = self.arena.get(node_idx)?;
        let start = self.line_map.offset_to_position(node.pos, self.source_text);
        let end = self.line_map.offset_to_position(node.end, self.source_text);

        Some(HoverInfo {
            contents,
            range: Some(Range::new(start, end)),
        })
    }

    fn format_jsdoc_for_hover(&self, doc: &str) -> Option<String> {
        if doc.is_empty() {
            return None;
        }

        let parsed = parse_jsdoc(doc);
        if parsed.is_empty() {
            return Some(doc.to_string());
        }

        let mut sections = Vec::new();
        if let Some(summary) = parsed.summary.as_ref() {
            if !summary.is_empty() {
                sections.push(summary.clone());
            }
        }

        if !parsed.params.is_empty() {
            let mut names: Vec<&String> = parsed.params.keys().collect();
            names.sort();
            let mut lines = Vec::new();
            lines.push("Parameters:".to_string());
            for name in names {
                let desc = parsed.params.get(name).map(|s| s.as_str()).unwrap_or("");
                if desc.is_empty() {
                    lines.push(format!("- `{}`", name));
                } else {
                    lines.push(format!("- `{}` {}", name, desc));
                }
            }
            sections.push(lines.join("\n"));
        }

        let formatted = sections.join("\n\n");
        if formatted.is_empty() {
            Some(doc.to_string())
        } else {
            Some(formatted)
        }
    }

    /// Helper to get a human-readable kind string for the symbol.
    fn get_symbol_kind_string(&self, symbol: &crate::binder::Symbol) -> &'static str {
        use crate::binder::symbol_flags;
        let f = symbol.flags;

        if f & symbol_flags::FUNCTION != 0 {
            "function"
        } else if f & symbol_flags::CLASS != 0 {
            "class"
        } else if f & symbol_flags::INTERFACE != 0 {
            "interface"
        } else if f & symbol_flags::REGULAR_ENUM != 0 {
            "enum"
        } else if f & symbol_flags::TYPE_ALIAS != 0 {
            "type"
        } else if f & (symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE) != 0 {
            "module"
        } else if f & symbol_flags::METHOD != 0 {
            "method"
        } else if f & symbol_flags::PROPERTY != 0 {
            "property"
        } else if f & symbol_flags::BLOCK_SCOPED_VARIABLE != 0 {
            "let/const"
        } else if f & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0 {
            "var"
        } else {
            "variable"
        }
    }
}

#[cfg(test)]
mod hover_tests {
    use super::*;
    use crate::binder::BinderState;
    use crate::lsp::position::LineMap;
    use crate::parser::ParserState;
    use crate::solver::TypeInterner;

    #[test]
    fn test_hover_variable_type() {
        // /** The answer */
        // const x = 42;
        // x;
        let source = "/** The answer */\nconst x = 42;\nx;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let interner = TypeInterner::new();
        let line_map = LineMap::build(source);

        let provider = HoverProvider::new(
            parser.get_arena(),
            &binder,
            &line_map,
            &interner,
            source,
            "test.ts".to_string(),
        );

        // Hover over 'x' in the last line (line 2, column 0)
        let pos = Position::new(2, 0);
        let mut cache = None;
        let info = provider.get_hover(root, pos, &mut cache);

        assert!(info.is_some(), "Should find hover info");

        if let Some(info) = info {
            // Check that we have contents
            assert!(!info.contents.is_empty(), "Should have contents");

            // First content should be the type signature
            assert!(
                info.contents[0].contains("x"),
                "Should contain variable name"
            );

            // Check that we have a range
            assert!(info.range.is_some(), "Should have range");
        }
    }

    #[test]
    fn test_hover_at_eof_identifier() {
        // /** The answer */
        // const x = 42;
        // x
        let source = "/** The answer */\nconst x = 42;\nx";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let interner = TypeInterner::new();
        let line_map = LineMap::build(source);

        let provider = HoverProvider::new(
            parser.get_arena(),
            &binder,
            &line_map,
            &interner,
            source,
            "test.ts".to_string(),
        );

        // Position at EOF, just after 'x' (line 2, column 1).
        let pos = Position::new(2, 1);
        let mut cache = None;
        let info = provider.get_hover(root, pos, &mut cache);

        assert!(info.is_some(), "Should find hover info at EOF");
        if let Some(info) = info {
            assert!(
                info.contents
                    .iter()
                    .any(|content| content.contains("The answer"))
            );
        }
    }

    #[test]
    fn test_hover_incomplete_member_access() {
        let source = "const foo = 1;\nfoo.";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let interner = TypeInterner::new();
        let line_map = LineMap::build(source);

        let provider = HoverProvider::new(
            parser.get_arena(),
            &binder,
            &line_map,
            &interner,
            source,
            "test.ts".to_string(),
        );

        let pos = Position::new(1, 4); // After the trailing dot.
        let mut cache = None;
        let info = provider.get_hover(root, pos, &mut cache);

        assert!(
            info.is_some(),
            "Should find hover info after incomplete member access"
        );
        if let Some(info) = info {
            assert!(
                info.contents[0].contains("foo"),
                "Should use base identifier for hover"
            );
        }
    }

    #[test]
    fn test_hover_jsdoc_summary_and_params() {
        let source = "/**\n * Adds two numbers.\n * @param a First number.\n * @param b Second number.\n */\nfunction add(a: number, b: number): number { return a + b; }\nadd(1, 2);";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let interner = TypeInterner::new();
        let line_map = LineMap::build(source);

        let provider = HoverProvider::new(
            parser.get_arena(),
            &binder,
            &line_map,
            &interner,
            source,
            "test.ts".to_string(),
        );

        let pos = Position::new(6, 0);
        let mut cache = None;
        let info = provider
            .get_hover(root, pos, &mut cache)
            .expect("Expected hover info");

        let doc = info
            .contents
            .iter()
            .find(|c| c.contains("Adds two numbers."))
            .cloned()
            .unwrap_or_default();
        assert!(doc.contains("Adds two numbers."));
        assert!(doc.contains("Parameters:"));
        assert!(doc.contains("`a` First number."));
        assert!(doc.contains("`b` Second number."));
    }

    #[test]
    fn test_hover_no_symbol() {
        let source = "const x = 42;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let interner = TypeInterner::new();
        let line_map = LineMap::build(source);

        let provider = HoverProvider::new(
            parser.get_arena(),
            &binder,
            &line_map,
            &interner,
            source,
            "test.ts".to_string(),
        );

        // Hover over semicolon (no symbol)
        let pos = Position::new(0, 13);
        let mut cache = None;
        let info = provider.get_hover(root, pos, &mut cache);

        assert!(info.is_none(), "Should not find hover info at semicolon");
    }

    #[test]
    fn test_hover_function() {
        let source = "function foo() { return 1; }\nfoo();";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let interner = TypeInterner::new();
        let line_map = LineMap::build(source);

        let provider = HoverProvider::new(
            parser.get_arena(),
            &binder,
            &line_map,
            &interner,
            source,
            "test.ts".to_string(),
        );

        // Hover over 'foo' in the call
        let pos = Position::new(1, 0);
        let mut cache = None;
        let info = provider.get_hover(root, pos, &mut cache);

        assert!(info.is_some(), "Should find hover info for function");

        if let Some(info) = info {
            assert!(
                info.contents[0].contains("foo"),
                "Should contain function name"
            );
        }
    }
}
