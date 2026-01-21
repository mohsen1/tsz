//! Document Symbols implementation for LSP.
//!
//! Provides an outline/structure view of a TypeScript file showing all
//! functions, classes, interfaces, types, variables, etc.

use crate::lsp::position::{LineMap, Position, Range};
use crate::parser::node::NodeArena;
use crate::parser::{NodeIndex, node_flags, syntax_kind_ext};
use crate::scanner::SyntaxKind;

/// A symbol kind (matches LSP SymbolKind values).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum SymbolKind {
    File = 1,
    Module = 2,
    Namespace = 3,
    Package = 4,
    Class = 5,
    Method = 6,
    Property = 7,
    Field = 8,
    Constructor = 9,
    Enum = 10,
    Interface = 11,
    Function = 12,
    Variable = 13,
    Constant = 14,
    String = 15,
    Number = 16,
    Boolean = 17,
    Array = 18,
    Object = 19,
    Key = 20,
    Null = 21,
    EnumMember = 22,
    Struct = 23,
    Event = 24,
    Operator = 25,
    TypeParameter = 26,
}

/// Represents programming constructs like variables, classes, interfaces, etc.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DocumentSymbol {
    /// The name of this symbol.
    pub name: String,
    /// More detail for this symbol, e.g. the signature of a function.
    pub detail: Option<String>,
    /// The kind of this symbol.
    pub kind: SymbolKind,
    /// The range enclosing this symbol (entire definition).
    pub range: Range,
    /// The range that should be selected and revealed when this symbol is being picked (just the identifier).
    pub selection_range: Range,
    /// Children of this symbol, e.g. properties of a class.
    pub children: Vec<DocumentSymbol>,
}

impl DocumentSymbol {
    /// Create a new document symbol.
    pub fn new(name: String, kind: SymbolKind, range: Range, selection_range: Range) -> Self {
        Self {
            name,
            detail: None,
            kind,
            range,
            selection_range,
            children: Vec::new(),
        }
    }

    /// Add a child symbol.
    pub fn add_child(&mut self, child: DocumentSymbol) {
        self.children.push(child);
    }

    /// Set the detail field.
    pub fn with_detail(mut self, detail: String) -> Self {
        self.detail = Some(detail);
        self
    }
}

/// Document symbol provider.
pub struct DocumentSymbolProvider<'a> {
    arena: &'a NodeArena,
    line_map: &'a LineMap,
    source_text: &'a str,
}

impl<'a> DocumentSymbolProvider<'a> {
    /// Create a new document symbol provider.
    pub fn new(arena: &'a NodeArena, line_map: &'a LineMap, source_text: &'a str) -> Self {
        Self {
            arena,
            line_map,
            source_text,
        }
    }

    /// Get all symbols in the document.
    pub fn get_document_symbols(&self, root: NodeIndex) -> Vec<DocumentSymbol> {
        self.collect_symbols(root)
    }

    /// Recursively collect symbols from a node.
    fn collect_symbols(&self, node_idx: NodeIndex) -> Vec<DocumentSymbol> {
        let Some(node) = self.arena.get(node_idx) else {
            return Vec::new();
        };

        match node.kind {
            // Source File: Recurse into statements
            k if k == syntax_kind_ext::SOURCE_FILE => {
                let mut symbols = Vec::new();
                if let Some(sf) = self.arena.get_source_file(node) {
                    for &stmt in &sf.statements.nodes {
                        symbols.extend(self.collect_symbols(stmt));
                    }
                }
                symbols
            }

            // Function Declaration
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = self.arena.get_function(node) {
                    let name_node = func.name;
                    let name = self
                        .get_name(name_node)
                        .unwrap_or_else(|| "<anonymous>".to_string());

                    let range = self.get_range(node_idx);
                    let selection_range = if !name_node.is_none() {
                        self.get_range(name_node)
                    } else {
                        self.get_range_keyword(node_idx, 8) // "function".len()
                    };

                    // Collect nested symbols (functions/classes inside this function)
                    let children = self.collect_children_from_block(func.body);

                    vec![DocumentSymbol {
                        name,
                        detail: None,
                        kind: SymbolKind::Function,
                        range,
                        selection_range,
                        children,
                    }]
                } else {
                    vec![]
                }
            }

            // Class Declaration
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class) = self.arena.get_class(node) {
                    let name_node = class.name;
                    let name = self
                        .get_name(name_node)
                        .unwrap_or_else(|| "<class>".to_string());

                    let range = self.get_range(node_idx);
                    let selection_range = if !name_node.is_none() {
                        self.get_range(name_node)
                    } else {
                        self.get_range_keyword(node_idx, 5) // "class".len()
                    };

                    let mut children = Vec::new();
                    for &member in &class.members.nodes {
                        children.extend(self.collect_symbols(member));
                    }

                    vec![DocumentSymbol {
                        name,
                        detail: None,
                        kind: SymbolKind::Class,
                        range,
                        selection_range,
                        children,
                    }]
                } else {
                    vec![]
                }
            }

            // Interface Declaration
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                if let Some(iface) = self.arena.get_interface(node) {
                    let name_node = iface.name;
                    let name = self
                        .get_name(name_node)
                        .unwrap_or_else(|| "<interface>".to_string());

                    let range = self.get_range(node_idx);
                    let selection_range = if !name_node.is_none() {
                        self.get_range(name_node)
                    } else {
                        self.get_range_keyword(node_idx, 9) // "interface".len()
                    };

                    let mut children = Vec::new();
                    for &member in &iface.members.nodes {
                        children.extend(self.collect_symbols(member));
                    }

                    vec![DocumentSymbol {
                        name,
                        detail: None,
                        kind: SymbolKind::Interface,
                        range,
                        selection_range,
                        children,
                    }]
                } else {
                    vec![]
                }
            }

            // Type Alias Declaration
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                if let Some(alias) = self.arena.get_type_alias(node) {
                    let name_node = alias.name;
                    let name = self
                        .get_name(name_node)
                        .unwrap_or_else(|| "<type>".to_string());

                    let range = self.get_range(node_idx);
                    let selection_range = if !name_node.is_none() {
                        self.get_range(name_node)
                    } else {
                        self.get_range_keyword(node_idx, 4) // "type".len()
                    };

                    vec![DocumentSymbol {
                        name,
                        detail: None,
                        kind: SymbolKind::TypeParameter,
                        range,
                        selection_range,
                        children: vec![],
                    }]
                } else {
                    vec![]
                }
            }

            // Variable Statement (can contain multiple declarations)
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                let mut symbols = Vec::new();
                if let Some(var) = self.arena.get_variable(node) {
                    // VARIABLE_STATEMENT -> VARIABLE_DECLARATION_LIST -> declarations
                    for &decl_list_idx in &var.declarations.nodes {
                        if let Some(list_node) = self.arena.get(decl_list_idx) {
                            // Check if this is const/let/var based on list node flags
                            let is_const = (list_node.flags as u32 & node_flags::CONST) != 0;
                            let kind = if is_const {
                                SymbolKind::Constant
                            } else {
                                SymbolKind::Variable
                            };

                            if let Some(list) = self.arena.get_variable(list_node) {
                                for &decl_idx in &list.declarations.nodes {
                                    if let Some(decl_node) = self.arena.get(decl_idx)
                                        && let Some(decl) =
                                            self.arena.get_variable_declaration(decl_node)
                                        && let Some(name) = self.get_name(decl.name)
                                    {
                                        let range = self.get_range(decl_idx);
                                        let selection_range = self.get_range(decl.name);

                                        symbols.push(DocumentSymbol {
                                            name,
                                            detail: None,
                                            kind,
                                            range,
                                            selection_range,
                                            children: vec![],
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                symbols
            }

            // Enum Declaration
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_decl) = self.arena.get_enum(node) {
                    let name_node = enum_decl.name;
                    let name = self
                        .get_name(name_node)
                        .unwrap_or_else(|| "<enum>".to_string());

                    let range = self.get_range(node_idx);
                    let selection_range = self.get_range(name_node);

                    let mut children = Vec::new();
                    for &member in &enum_decl.members.nodes {
                        children.extend(self.collect_symbols(member));
                    }

                    vec![DocumentSymbol {
                        name,
                        detail: None,
                        kind: SymbolKind::Enum,
                        range,
                        selection_range,
                        children,
                    }]
                } else {
                    vec![]
                }
            }

            // Enum Member
            k if k == syntax_kind_ext::ENUM_MEMBER => {
                if let Some(member) = self.arena.get_enum_member(node) {
                    let name_node = member.name;
                    let name = self
                        .get_name(name_node)
                        .unwrap_or_else(|| "<member>".to_string());

                    let range = self.get_range(node_idx);
                    let selection_range = self.get_range(name_node);

                    vec![DocumentSymbol {
                        name,
                        detail: None,
                        kind: SymbolKind::EnumMember,
                        range,
                        selection_range,
                        children: vec![],
                    }]
                } else {
                    vec![]
                }
            }

            // Method Declaration (Class Member)
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.arena.get_method_decl(node) {
                    let name = self
                        .get_name(method.name)
                        .unwrap_or_else(|| "<method>".to_string());
                    let range = self.get_range(node_idx);
                    let selection_range = self.get_range(method.name);

                    vec![DocumentSymbol {
                        name,
                        detail: None,
                        kind: SymbolKind::Method,
                        range,
                        selection_range,
                        children: vec![],
                    }]
                } else {
                    vec![]
                }
            }

            // Property Declaration (Class Member)
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                if let Some(prop) = self.arena.get_property_decl(node) {
                    let name = self
                        .get_name(prop.name)
                        .unwrap_or_else(|| "<property>".to_string());
                    let range = self.get_range(node_idx);
                    let selection_range = self.get_range(prop.name);

                    vec![DocumentSymbol {
                        name,
                        detail: None,
                        kind: SymbolKind::Property,
                        range,
                        selection_range,
                        children: vec![],
                    }]
                } else {
                    vec![]
                }
            }

            // Constructor (Class Member)
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                let children = if let Some(ctor) = self.arena.get_constructor(node) {
                    self.collect_children_from_block(ctor.body)
                } else {
                    vec![]
                };

                vec![DocumentSymbol {
                    name: "constructor".to_string(),
                    detail: None,
                    kind: SymbolKind::Constructor,
                    range: self.get_range(node_idx),
                    selection_range: self.get_range_keyword(node_idx, 11), // "constructor".len()
                    children,
                }]
            }

            // Accessors (Class Members)
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(node) {
                    let name_node = accessor.name;
                    let name = self
                        .get_name(name_node)
                        .unwrap_or_else(|| "<accessor>".to_string());
                    let range = self.get_range(node_idx);
                    let selection_range = self.get_range(name_node);

                    vec![DocumentSymbol {
                        name,
                        detail: None,
                        kind: SymbolKind::Property,
                        range,
                        selection_range,
                        children: vec![],
                    }]
                } else {
                    vec![]
                }
            }

            // Module / Namespace Declaration
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                if let Some(module) = self.arena.get_module(node) {
                    let name = self
                        .get_name(module.name)
                        .unwrap_or_else(|| "<module>".to_string());
                    let range = self.get_range(node_idx);
                    let selection_range = self.get_range(module.name);

                    let children = if !module.body.is_none() {
                        self.collect_symbols(module.body)
                    } else {
                        vec![]
                    };

                    vec![DocumentSymbol {
                        name,
                        detail: None,
                        kind: SymbolKind::Module,
                        range,
                        selection_range,
                        children,
                    }]
                } else {
                    vec![]
                }
            }

            // Module Block (body of a namespace)
            k if k == syntax_kind_ext::MODULE_BLOCK => {
                if let Some(block) = self.arena.get_module_block(node) {
                    let mut symbols = Vec::new();
                    if let Some(stmts) = &block.statements {
                        for &stmt in &stmts.nodes {
                            symbols.extend(self.collect_symbols(stmt));
                        }
                    }
                    symbols
                } else {
                    vec![]
                }
            }

            // Export Declaration - recurse into the exported clause
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                if let Some(export) = self.arena.get_export_decl(node)
                    && !export.export_clause.is_none()
                {
                    // Check if this is an exported declaration (export function foo() {})
                    if let Some(clause_node) = self.arena.get(export.export_clause) {
                        // If it's a declaration, collect it
                        if self.is_declaration(clause_node.kind) {
                            return self.collect_symbols(export.export_clause);
                        }
                    }
                }
                vec![]
            }

            // Default fallback
            _ => vec![],
        }
    }

    /// Helper to collect children from a block (e.g. inside function).
    /// Only collects nested functions/classes for the outline.
    fn collect_children_from_block(&self, block_idx: NodeIndex) -> Vec<DocumentSymbol> {
        let mut symbols = Vec::new();
        if block_idx.is_none() {
            return symbols;
        }

        if let Some(node) = self.arena.get(block_idx)
            && node.kind == syntax_kind_ext::BLOCK
            && let Some(block) = self.arena.get_block(node)
        {
            for &stmt in &block.statements.nodes {
                // Only collect declarations (functions, classes) - not variables
                if let Some(stmt_node) = self.arena.get(stmt)
                    && (stmt_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                        || stmt_node.kind == syntax_kind_ext::CLASS_DECLARATION)
                {
                    symbols.extend(self.collect_symbols(stmt));
                }
            }
        }
        symbols
    }

    /// Check if a node kind is a declaration.
    fn is_declaration(&self, kind: u16) -> bool {
        kind == syntax_kind_ext::FUNCTION_DECLARATION
            || kind == syntax_kind_ext::CLASS_DECLARATION
            || kind == syntax_kind_ext::VARIABLE_STATEMENT
            || kind == syntax_kind_ext::INTERFACE_DECLARATION
            || kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
            || kind == syntax_kind_ext::ENUM_DECLARATION
    }

    /// Convert node range to LSP Range.
    fn get_range(&self, node_idx: NodeIndex) -> Range {
        if let Some(node) = self.arena.get(node_idx) {
            let start = self.line_map.offset_to_position(node.pos, self.source_text);
            let end = self.line_map.offset_to_position(node.end, self.source_text);
            Range::new(start, end)
        } else {
            Range::new(Position::new(0, 0), Position::new(0, 0))
        }
    }

    /// Get range for a keyword (when no identifier exists, e.g. "constructor").
    fn get_range_keyword(&self, node_idx: NodeIndex, len: u32) -> Range {
        if let Some(node) = self.arena.get(node_idx) {
            let start = self.line_map.offset_to_position(node.pos, self.source_text);
            let end = self
                .line_map
                .offset_to_position(node.pos + len, self.source_text);
            Range::new(start, end)
        } else {
            Range::new(Position::new(0, 0), Position::new(0, 0))
        }
    }

    /// Extract text from identifier node.
    fn get_name(&self, node_idx: NodeIndex) -> Option<String> {
        if node_idx.is_none() {
            return None;
        }
        if let Some(node) = self.arena.get(node_idx) {
            if node.kind == SyntaxKind::Identifier as u16 {
                return self
                    .arena
                    .get_identifier(node)
                    .map(|id| id.escaped_text.clone());
            } else if node.kind == SyntaxKind::StringLiteral as u16
                || node.kind == SyntaxKind::NumericLiteral as u16
            {
                return self.arena.get_literal(node).map(|l| l.text.clone());
            }
        }
        None
    }
}

#[cfg(test)]
mod document_symbols_tests {
    use super::*;
    use crate::lsp::position::LineMap;
    use crate::parser::ParserState;

    #[test]
    fn test_document_symbols_class_with_members() {
        let source = "class Foo {\n  bar() {}\n  prop: number;\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let line_map = LineMap::build(source);

        let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
        let symbols = provider.get_document_symbols(root);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Foo");
        assert_eq!(symbols[0].kind, SymbolKind::Class);
        assert_eq!(symbols[0].children.len(), 2); // bar, prop

        assert_eq!(symbols[0].children[0].name, "bar");
        assert_eq!(symbols[0].children[0].kind, SymbolKind::Method);

        assert_eq!(symbols[0].children[1].name, "prop");
        assert_eq!(symbols[0].children[1].kind, SymbolKind::Property);
    }

    #[test]
    fn test_document_symbols_function_and_variable() {
        let source = "function baz() {}\nconst x = 1;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let line_map = LineMap::build(source);

        let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
        let symbols = provider.get_document_symbols(root);

        assert_eq!(symbols.len(), 2);

        assert_eq!(symbols[0].name, "baz");
        assert_eq!(symbols[0].kind, SymbolKind::Function);

        assert_eq!(symbols[1].name, "x");
        assert_eq!(symbols[1].kind, SymbolKind::Constant);
    }

    #[test]
    fn test_document_symbols_interface() {
        let source = "interface Point {\n  x: number;\n  y: number;\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let line_map = LineMap::build(source);

        let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
        let symbols = provider.get_document_symbols(root);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Point");
        assert_eq!(symbols[0].kind, SymbolKind::Interface);
    }

    #[test]
    fn test_document_symbols_enum() {
        let source = "enum Color { Red, Green, Blue }";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let line_map = LineMap::build(source);

        let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
        let symbols = provider.get_document_symbols(root);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Color");
        assert_eq!(symbols[0].kind, SymbolKind::Enum);
        assert_eq!(symbols[0].children.len(), 3);

        assert_eq!(symbols[0].children[0].name, "Red");
        assert_eq!(symbols[0].children[0].kind, SymbolKind::EnumMember);
    }

    #[test]
    fn test_document_symbols_multiple_variables() {
        let source = "const a = 1, b = 2;\nlet c = 3;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let line_map = LineMap::build(source);

        let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
        let symbols = provider.get_document_symbols(root);

        // Should have 3 symbols: a (const), b (const), c (var)
        assert_eq!(symbols.len(), 3);
        assert_eq!(symbols[0].name, "a");
        assert_eq!(symbols[0].kind, SymbolKind::Constant);
        assert_eq!(symbols[1].name, "b");
        assert_eq!(symbols[1].kind, SymbolKind::Constant);
        assert_eq!(symbols[2].name, "c");
        assert_eq!(symbols[2].kind, SymbolKind::Variable);
    }
}
