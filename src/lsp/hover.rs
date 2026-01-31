//! Hover implementation for LSP.
//!
//! Displays type information and documentation for the symbol at the cursor.
//! Produces quickinfo output compatible with tsserver's expected format:
//! - `display_string`: The raw signature (e.g. `const x: number`, `function foo(): void`)
//! - `kind`: The symbol kind (e.g. `const`, `function`, `class`)
//! - `kind_modifiers`: Comma-separated modifier list (e.g. `export,declare`)
//! - `documentation`: Extracted JSDoc content

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
    /// The raw display string for tsserver quickinfo (e.g. `const x: number`)
    pub display_string: String,
    /// The symbol kind string for tsserver (e.g. `const`, `function`, `class`)
    pub kind: String,
    /// Comma-separated kind modifiers for tsserver (e.g. `export,declare`)
    pub kind_modifiers: String,
    /// The documentation text extracted from JSDoc
    pub documentation: String,
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
        scope_stats: Option<&mut ScopeCacheStats>,
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
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let symbol_id = if let Some(scope_cache) = scope_cache {
            walker.resolve_node_cached(root, node_idx, scope_cache, scope_stats)?
        } else {
            walker.resolve_node(root, node_idx)?
        };
        let symbol = self.binder.symbols.get(symbol_id)?;

        // 3. Compute Type Information
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

        // 4. Get the declaration node for determining keyword and modifiers
        let decl_node_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else if let Some(&first) = symbol.declarations.first() {
            first
        } else {
            NodeIndex::NONE
        };

        // 5. Determine the kind string (tsserver-compatible)
        let kind = self.get_tsserver_kind(symbol, decl_node_idx);

        // 6. Determine kind modifiers (export, declare, abstract, etc.)
        let kind_modifiers = self.get_kind_modifiers(symbol, decl_node_idx);

        // 7. Construct the display string matching tsserver format
        let display_string = self.build_display_string(symbol, &kind, &type_string, decl_node_idx);

        // 8. Extract Documentation (JSDoc)
        let raw_documentation = if !decl_node_idx.is_none() {
            jsdoc_for_node(self.arena, root, decl_node_idx, self.source_text)
        } else {
            String::new()
        };
        let formatted_doc = self.format_jsdoc_for_hover(&raw_documentation);
        let documentation_text = self.extract_plain_documentation(&raw_documentation);

        // 9. Build response
        let mut contents = Vec::new();

        // Code block for the signature
        contents.push(format!("```typescript\n{}\n```", display_string));

        // Documentation paragraph
        if let Some(doc) = formatted_doc {
            contents.push(doc);
        }

        // Calculate range for the hovered identifier
        let node = self.arena.get(node_idx)?;
        let start = self.line_map.offset_to_position(node.pos, self.source_text);
        let end = self.line_map.offset_to_position(node.end, self.source_text);

        Some(HoverInfo {
            contents,
            range: Some(Range::new(start, end)),
            display_string,
            kind,
            kind_modifiers,
            documentation: documentation_text,
        })
    }

    /// Build the display string in tsserver quickinfo format.
    fn build_display_string(
        &self,
        symbol: &crate::binder::Symbol,
        kind: &str,
        type_string: &str,
        decl_node_idx: NodeIndex,
    ) -> String {
        use crate::binder::symbol_flags;
        let f = symbol.flags;

        if f & symbol_flags::FUNCTION != 0 {
            return format!("function {}{}", symbol.escaped_name, type_string);
        }
        if f & symbol_flags::CLASS != 0 {
            return format!("class {}", symbol.escaped_name);
        }
        if f & symbol_flags::INTERFACE != 0 {
            return format!("interface {}", symbol.escaped_name);
        }
        if f & symbol_flags::ENUM != 0 {
            return format!("enum {}", symbol.escaped_name);
        }
        if f & symbol_flags::TYPE_ALIAS != 0 {
            return format!("type {} = {}", symbol.escaped_name, type_string);
        }
        if f & symbol_flags::ENUM_MEMBER != 0 {
            let parent_name = self.get_parent_name(decl_node_idx);
            if let Some(parent) = parent_name {
                return format!(
                    "(enum member) {}.{} = {}",
                    parent, symbol.escaped_name, type_string
                );
            }
            return format!("(enum member) {} = {}", symbol.escaped_name, type_string);
        }
        if f & symbol_flags::PROPERTY != 0 {
            let parent_name = self.get_parent_name(decl_node_idx);
            if let Some(parent) = parent_name {
                return format!(
                    "(property) {}.{}: {}",
                    parent, symbol.escaped_name, type_string
                );
            }
            return format!("(property) {}: {}", symbol.escaped_name, type_string);
        }
        if f & symbol_flags::METHOD != 0 {
            let parent_name = self.get_parent_name(decl_node_idx);
            if let Some(parent) = parent_name {
                return format!("(method) {}.{}{}", parent, symbol.escaped_name, type_string);
            }
            return format!("(method) {}{}", symbol.escaped_name, type_string);
        }
        if f & (symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE) != 0 {
            return format!("namespace {}", symbol.escaped_name);
        }
        if f & symbol_flags::BLOCK_SCOPED_VARIABLE != 0 {
            let keyword = self.get_variable_keyword(decl_node_idx);
            return format!("{} {}: {}", keyword, symbol.escaped_name, type_string);
        }
        if f & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0 {
            if self.is_parameter_declaration(decl_node_idx) {
                return format!("(parameter) {}: {}", symbol.escaped_name, type_string);
            }
            return format!("var {}: {}", symbol.escaped_name, type_string);
        }

        format!("({}) {}: {}", kind, symbol.escaped_name, type_string)
    }

    /// Get the tsserver-compatible kind string for the symbol.
    fn get_tsserver_kind(
        &self,
        symbol: &crate::binder::Symbol,
        decl_node_idx: NodeIndex,
    ) -> String {
        use crate::binder::symbol_flags;
        let f = symbol.flags;

        if f & symbol_flags::FUNCTION != 0 {
            return "function".to_string();
        }
        if f & symbol_flags::CLASS != 0 {
            return "class".to_string();
        }
        if f & symbol_flags::INTERFACE != 0 {
            return "interface".to_string();
        }
        if f & symbol_flags::ENUM != 0 {
            return "enum".to_string();
        }
        if f & symbol_flags::TYPE_ALIAS != 0 {
            return "type".to_string();
        }
        if f & symbol_flags::ENUM_MEMBER != 0 {
            return "enum member".to_string();
        }
        if f & (symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE) != 0 {
            return "module".to_string();
        }
        if f & symbol_flags::METHOD != 0 {
            return "method".to_string();
        }
        if f & symbol_flags::CONSTRUCTOR != 0 {
            return "constructor".to_string();
        }
        if f & symbol_flags::PROPERTY != 0 {
            return "property".to_string();
        }
        if f & symbol_flags::TYPE_PARAMETER != 0 {
            return "type parameter".to_string();
        }
        if f & symbol_flags::GET_ACCESSOR != 0 {
            return "getter".to_string();
        }
        if f & symbol_flags::SET_ACCESSOR != 0 {
            return "setter".to_string();
        }
        if f & symbol_flags::BLOCK_SCOPED_VARIABLE != 0 {
            return self.get_variable_keyword(decl_node_idx).to_string();
        }
        if f & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0 {
            if self.is_parameter_declaration(decl_node_idx) {
                return "parameter".to_string();
            }
            return "var".to_string();
        }
        "var".to_string()
    }

    /// Get comma-separated kind modifiers string for tsserver.
    fn get_kind_modifiers(
        &self,
        symbol: &crate::binder::Symbol,
        decl_node_idx: NodeIndex,
    ) -> String {
        use crate::binder::symbol_flags as sf;
        use crate::parser::modifier_flags as mf;

        let mut modifiers = Vec::new();

        if symbol.is_exported || symbol.flags & sf::EXPORT_VALUE != 0 {
            modifiers.push("export");
        }
        if symbol.flags & sf::ABSTRACT != 0 {
            modifiers.push("abstract");
        }
        if symbol.flags & sf::STATIC != 0 {
            modifiers.push("static");
        }
        if symbol.flags & sf::PRIVATE != 0 {
            modifiers.push("private");
        }
        if symbol.flags & sf::PROTECTED != 0 {
            modifiers.push("protected");
        }

        if !decl_node_idx.is_none() {
            if let Some(ext) = self.arena.get_extended(decl_node_idx) {
                let mflags = ext.modifier_flags;
                if mflags & mf::AMBIENT != 0 {
                    modifiers.push("declare");
                }
                if mflags & mf::ASYNC != 0 {
                    modifiers.push("async");
                }
                if mflags & mf::READONLY != 0 {
                    modifiers.push("readonly");
                }
                if !modifiers.contains(&"export") && mflags & mf::EXPORT != 0 {
                    modifiers.push("export");
                }
                if !modifiers.contains(&"abstract") && mflags & mf::ABSTRACT != 0 {
                    modifiers.push("abstract");
                }
            }
        }

        modifiers.join(",")
    }

    /// Determine the variable keyword (const, let, or var) from the declaration node.
    fn get_variable_keyword(&self, decl_node_idx: NodeIndex) -> &'static str {
        use crate::parser::flags::node_flags;
        use crate::parser::syntax_kind_ext;

        if decl_node_idx.is_none() {
            return "let";
        }

        let node = match self.arena.get(decl_node_idx) {
            Some(n) => n,
            None => return "let",
        };

        let list_idx = if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            if let Some(ext) = self.arena.get_extended(decl_node_idx) {
                ext.parent
            } else {
                return "let";
            }
        } else if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            decl_node_idx
        } else {
            let flags = node.flags as u32;
            if flags & node_flags::CONST != 0 {
                return "const";
            }
            if flags & node_flags::LET != 0 {
                return "let";
            }
            return "var";
        };

        if let Some(list_node) = self.arena.get(list_idx) {
            let flags = list_node.flags as u32;
            if flags & node_flags::CONST != 0 {
                return "const";
            }
            if flags & node_flags::LET != 0 {
                return "let";
            }
        }

        "let"
    }

    /// Check if a declaration node is a parameter.
    fn is_parameter_declaration(&self, decl_node_idx: NodeIndex) -> bool {
        use crate::parser::syntax_kind_ext;

        if decl_node_idx.is_none() {
            return false;
        }
        if let Some(node) = self.arena.get(decl_node_idx) {
            return node.kind == syntax_kind_ext::PARAMETER;
        }
        false
    }

    /// Get the parent symbol name (for enum members, properties, methods).
    fn get_parent_name(&self, decl_node_idx: NodeIndex) -> Option<String> {
        if decl_node_idx.is_none() {
            return None;
        }
        let ext = self.arena.get_extended(decl_node_idx)?;
        let parent_idx = ext.parent;
        if parent_idx.is_none() {
            return None;
        }
        let parent_node = self.arena.get(parent_idx)?;
        if let Some(data) = self.arena.get_identifier(parent_node) {
            return Some(self.arena.resolve_identifier_text(data).to_string());
        }
        if let Some(data) = self.arena.get_class(parent_node) {
            if let Some(name_node) = self.arena.get(data.name) {
                if let Some(id) = self.arena.get_identifier(name_node) {
                    return Some(self.arena.resolve_identifier_text(id).to_string());
                }
            }
        }
        if let Some(data) = self.arena.get_enum(parent_node) {
            if let Some(name_node) = self.arena.get(data.name) {
                if let Some(id) = self.arena.get_identifier(name_node) {
                    return Some(self.arena.resolve_identifier_text(id).to_string());
                }
            }
        }
        if let Some(data) = self.arena.get_interface(parent_node) {
            if let Some(name_node) = self.arena.get(data.name) {
                if let Some(id) = self.arena.get_identifier(name_node) {
                    return Some(self.arena.resolve_identifier_text(id).to_string());
                }
            }
        }
        None
    }

    /// Extract plain documentation text from JSDoc (without markdown formatting).
    fn extract_plain_documentation(&self, doc: &str) -> String {
        if doc.is_empty() {
            return String::new();
        }
        let parsed = parse_jsdoc(doc);
        if let Some(summary) = parsed.summary.as_ref() {
            summary.clone()
        } else {
            doc.to_string()
        }
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
        if let Some(summary) = parsed.summary.as_ref()
            && !summary.is_empty()
        {
            sections.push(summary.clone());
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
}

#[cfg(test)]
mod hover_tests {
    use super::*;
    use crate::binder::BinderState;
    use crate::lsp::position::LineMap;
    use crate::parser::ParserState;
    use crate::solver::TypeInterner;

    /// Helper to set up hover infrastructure and get hover info at a position.
    fn get_hover_at(source: &str, line: u32, col: u32) -> Option<HoverInfo> {
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

        let pos = Position::new(line, col);
        let mut cache = None;
        provider.get_hover(root, pos, &mut cache)
    }

    #[test]
    #[ignore] // TODO: Fix this test
    fn test_hover_variable_type() {
        let source = "/** The answer */\nconst x = 42;\nx;";
        let info = get_hover_at(source, 2, 0);
        assert!(info.is_some(), "Should find hover info");
        if let Some(info) = info {
            assert!(!info.contents.is_empty(), "Should have contents");
            assert!(
                info.contents[0].contains("x"),
                "Should contain variable name"
            );
            assert!(info.range.is_some(), "Should have range");
        }
    }

    #[test]
    #[ignore] // TODO: Fix this test
    fn test_hover_at_eof_identifier() {
        let source = "/** The answer */\nconst x = 42;\nx";
        let info = get_hover_at(source, 2, 1);
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
    #[ignore] // TODO: Fix this test
    fn test_hover_incomplete_member_access() {
        let source = "const foo = 1;\nfoo.";
        let info = get_hover_at(source, 1, 4);
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
    #[ignore] // TODO: Fix this test
    fn test_hover_jsdoc_summary_and_params() {
        let source = "/**\n * Adds two numbers.\n * @param a First number.\n * @param b Second number.\n */\nfunction add(a: number, b: number): number { return a + b; }\nadd(1, 2);";
        let info = get_hover_at(source, 6, 0).expect("Expected hover info");
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
        let info = get_hover_at(source, 0, 13);
        assert!(info.is_none(), "Should not find hover info at semicolon");
    }

    #[test]
    #[ignore] // TODO: Fix this test
    fn test_hover_function() {
        let source = "function foo() { return 1; }\nfoo();";
        let info = get_hover_at(source, 1, 0);
        assert!(info.is_some(), "Should find hover info for function");
        if let Some(info) = info {
            assert!(
                info.contents[0].contains("foo"),
                "Should contain function name"
            );
        }
    }

    // =========================================================================
    // New tests for tsserver-compatible quickinfo format
    // =========================================================================

    #[test]
    #[ignore] // TODO: Fix this test
    fn test_hover_const_variable_display_string() {
        let source = "const x = 42;\nx;";
        let info = get_hover_at(source, 1, 0).expect("Should find hover info");
        assert!(
            info.display_string.starts_with("const ") || info.display_string.starts_with("let "),
            "Variable display_string should start with const or let keyword, got: {}",
            info.display_string
        );
        assert!(
            info.display_string.contains("x"),
            "display_string should contain variable name 'x', got: {}",
            info.display_string
        );
        assert!(
            info.display_string.contains(':'),
            "display_string should contain colon for type annotation, got: {}",
            info.display_string
        );
        assert!(
            info.kind == "const" || info.kind == "let",
            "Kind should be 'const' or 'let' for block-scoped variable, got: {}",
            info.kind
        );
    }

    #[test]
    #[ignore] // TODO: Fix this test
    fn test_hover_let_variable_display_string() {
        let source = "let y = \"hello\";\ny;";
        let info = get_hover_at(source, 1, 0).expect("Should find hover info");
        assert!(
            info.display_string.starts_with("let "),
            "Let variable display_string should start with 'let ', got: {}",
            info.display_string
        );
        assert!(
            info.display_string.contains("y"),
            "display_string should contain variable name 'y', got: {}",
            info.display_string
        );
        assert_eq!(
            info.kind, "let",
            "Kind should be 'let' for let variable, got: {}",
            info.kind
        );
    }

    #[test]
    #[ignore] // TODO: Fix this test
    fn test_hover_var_variable_display_string() {
        let source = "var z = true;\nz;";
        let info = get_hover_at(source, 1, 0).expect("Should find hover info");
        assert!(
            info.display_string.starts_with("var "),
            "Var variable display_string should start with 'var ', got: {}",
            info.display_string
        );
        assert!(
            info.display_string.contains("z"),
            "display_string should contain variable name 'z', got: {}",
            info.display_string
        );
        assert_eq!(
            info.kind, "var",
            "Kind should be 'var' for var variable, got: {}",
            info.kind
        );
    }

    #[test]
    #[ignore] // TODO: Fix this test
    fn test_hover_function_display_string() {
        let source = "function greet(name: string): void {}\ngreet(\"hi\");";
        let info = get_hover_at(source, 1, 0).expect("Should find hover info");
        assert!(
            info.display_string.starts_with("function "),
            "Function display_string should start with 'function ', got: {}",
            info.display_string
        );
        assert!(
            info.display_string.contains("greet"),
            "display_string should contain function name 'greet', got: {}",
            info.display_string
        );
        assert_eq!(
            info.kind, "function",
            "Kind should be 'function', got: {}",
            info.kind
        );
    }

    #[test]
    #[ignore] // TODO: Fix this test
    fn test_hover_class_display_string() {
        let source = "class MyClass { x: number = 0; }\nlet c = new MyClass();";
        let info = get_hover_at(source, 0, 6).expect("Should find hover info for class");
        assert!(
            info.display_string.starts_with("class "),
            "Class display_string should start with 'class ', got: {}",
            info.display_string
        );
        assert!(
            info.display_string.contains("MyClass"),
            "display_string should contain class name, got: {}",
            info.display_string
        );
        assert_eq!(
            info.kind, "class",
            "Kind should be 'class', got: {}",
            info.kind
        );
    }

    #[test]
    #[ignore] // TODO: Fix this test
    fn test_hover_interface_display_string() {
        let source = "interface IPoint { x: number; y: number; }\nlet p: IPoint;";
        let info = get_hover_at(source, 0, 10).expect("Should find hover info for interface");
        assert!(
            info.display_string.starts_with("interface "),
            "Interface display_string should start with 'interface ', got: {}",
            info.display_string
        );
        assert!(
            info.display_string.contains("IPoint"),
            "display_string should contain interface name, got: {}",
            info.display_string
        );
        assert_eq!(
            info.kind, "interface",
            "Kind should be 'interface', got: {}",
            info.kind
        );
    }

    #[test]
    #[ignore] // TODO: Fix this test
    fn test_hover_enum_display_string() {
        let source = "enum Color { Red, Green, Blue }\nlet c: Color;";
        let info = get_hover_at(source, 0, 5).expect("Should find hover info for enum");
        assert!(
            info.display_string.starts_with("enum "),
            "Enum display_string should start with 'enum ', got: {}",
            info.display_string
        );
        assert!(
            info.display_string.contains("Color"),
            "display_string should contain enum name, got: {}",
            info.display_string
        );
        assert_eq!(
            info.kind, "enum",
            "Kind should be 'enum', got: {}",
            info.kind
        );
    }

    #[test]
    #[ignore] // TODO: Fix this test
    fn test_hover_kind_field_populated() {
        let source = "const a = 1;\nlet b = 2;\nfunction f() {}\nclass C {}\ninterface I {}\na; b;";
        let info_a = get_hover_at(source, 5, 0).expect("Should find hover info for a");
        assert!(
            !info_a.kind.is_empty(),
            "Kind should not be empty for const variable"
        );
        let info_b = get_hover_at(source, 5, 3).expect("Should find hover info for b");
        assert!(
            !info_b.kind.is_empty(),
            "Kind should not be empty for let variable"
        );
    }

    #[test]
    #[ignore] // TODO: Fix this test
    fn test_hover_documentation_field_with_jsdoc() {
        let source = "/** My variable */\nconst x = 42;\nx;";
        let info = get_hover_at(source, 2, 0).expect("Should find hover info");
        assert!(
            info.documentation.contains("My variable"),
            "documentation field should contain JSDoc summary, got: '{}'",
            info.documentation
        );
    }

    #[test]
    #[ignore] // TODO: Fix this test
    fn test_hover_documentation_field_empty_without_jsdoc() {
        let source = "const x = 42;\nx;";
        let info = get_hover_at(source, 1, 0).expect("Should find hover info");
        assert!(
            info.documentation.is_empty(),
            "documentation field should be empty without JSDoc, got: '{}'",
            info.documentation
        );
    }

    #[test]
    #[ignore] // TODO: Fix this test
    fn test_hover_display_string_in_code_block() {
        let source = "const x = 42;\nx;";
        let info = get_hover_at(source, 1, 0).expect("Should find hover info");
        assert!(
            info.contents[0].contains(&info.display_string),
            "Code block should contain the display_string. Code block: '{}', display_string: '{}'",
            info.contents[0],
            info.display_string
        );
    }

    #[test]
    #[ignore] // TODO: Fix this test
    fn test_hover_type_alias_display_string() {
        let source = "type MyStr = string;\nlet s: MyStr;";
        let info = get_hover_at(source, 0, 5).expect("Should find hover info for type alias");
        assert!(
            info.display_string.starts_with("type "),
            "Type alias display_string should start with 'type ', got: {}",
            info.display_string
        );
        assert!(
            info.display_string.contains("MyStr"),
            "display_string should contain type alias name, got: {}",
            info.display_string
        );
        assert_eq!(
            info.kind, "type",
            "Kind should be 'type', got: {}",
            info.kind
        );
    }
}
