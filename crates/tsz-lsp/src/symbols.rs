//! Document Symbols API
//!
//! Provides hierarchical symbol tree extraction from TypeScript AST.
//! This is the main public interface for document symbol functionality.
//!
//! # Example
//! ```ignore
//! use wasm::lsp::symbols::{DocumentSymbols, SymbolKind};
//! use wasm::parser::ParserState;
//!
//! let source = "function foo() {}";
//! let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
//! let root = parser.parse_source_file();
//!
//! let symbols = DocumentSymbols::new(parser.get_arena(), source);
//! let tree = symbols.get_symbol_tree(root);
//! ```

pub use super::document_symbols::{DocumentSymbol, DocumentSymbolProvider, SymbolKind};

/// Main API for extracting document symbols from AST.
///
/// This provides a simplified interface to the DocumentSymbolProvider.
pub struct DocumentSymbols<'a> {
    arena: &'a tsz_parser::parser::node::NodeArena,
    line_map: tsz_common::position::LineMap,
    source_text: &'a str,
}

impl<'a> DocumentSymbols<'a> {
    /// Create a new document symbols extractor.
    ///
    /// # Arguments
    /// * `arena` - The AST node arena
    /// * `source_text` - The source code text
    pub fn new(arena: &'a tsz_parser::parser::node::NodeArena, source_text: &'a str) -> Self {
        let line_map = tsz_common::position::LineMap::build(source_text);
        Self {
            arena,
            line_map,
            source_text,
        }
    }

    /// Extract all symbols from the AST as a hierarchical tree.
    ///
    /// Returns a vector of top-level symbols (functions, classes, variables, etc.)
    /// with nested symbols as children.
    ///
    /// # Returns
    /// A vector of `DocumentSymbol` objects representing the symbol tree.
    pub fn get_symbol_tree(&self, root: tsz_parser::NodeIndex) -> Vec<DocumentSymbol> {
        let provider = DocumentSymbolProvider::new(self.arena, &self.line_map, self.source_text);
        provider.get_document_symbols(root)
    }
}

#[cfg(test)]
mod symbols_tests {
    use super::*;
    use tsz_parser::ParserState;

    #[test]
    fn test_symbols_api_simple() {
        let source = "function foo() {}\nconst x = 1;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let symbols = DocumentSymbols::new(parser.get_arena(), source);
        let tree = symbols.get_symbol_tree(root);

        assert_eq!(tree.len(), 2);
        assert_eq!(tree[0].name, "foo");
        assert_eq!(tree[0].kind, SymbolKind::Function);
        assert_eq!(tree[1].name, "x");
        assert_eq!(tree[1].kind, SymbolKind::Constant);
    }

    #[test]
    fn test_symbols_api_hierarchical() {
        let source = r#"
class MyClass {
    method1() {}
    property1: number;
}
"#;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let symbols = DocumentSymbols::new(parser.get_arena(), source);
        let tree = symbols.get_symbol_tree(root);

        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "MyClass");
        assert_eq!(tree[0].kind, SymbolKind::Class);
        assert_eq!(tree[0].children.len(), 2);
        assert_eq!(tree[0].children[0].name, "method1");
        assert_eq!(tree[0].children[0].kind, SymbolKind::Method);
        assert_eq!(tree[0].children[1].name, "property1");
        assert_eq!(tree[0].children[1].kind, SymbolKind::Property);
    }

    #[test]
    fn test_symbols_api_interface() {
        let source = r#"
interface Point {
    x: number;
    y: number;
}
"#;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let symbols = DocumentSymbols::new(parser.get_arena(), source);
        let tree = symbols.get_symbol_tree(root);

        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "Point");
        assert_eq!(tree[0].kind, SymbolKind::Interface);
        // Note: interface properties are not currently extracted as child symbols
    }

    #[test]
    fn test_symbols_api_enum() {
        let source = "enum Color { Red, Green, Blue }";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let symbols = DocumentSymbols::new(parser.get_arena(), source);
        let tree = symbols.get_symbol_tree(root);

        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "Color");
        assert_eq!(tree[0].kind, SymbolKind::Enum);
        assert_eq!(tree[0].children.len(), 3);
        assert_eq!(tree[0].children[0].name, "Red");
        assert_eq!(tree[0].children[0].kind, SymbolKind::EnumMember);
    }

    #[test]
    fn test_symbols_api_namespace() {
        let source = r#"
namespace MyNamespace {
    function foo() {}
    const bar = 1;
}
"#;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let symbols = DocumentSymbols::new(parser.get_arena(), source);
        let tree = symbols.get_symbol_tree(root);

        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "MyNamespace");
        assert_eq!(tree[0].kind, SymbolKind::Module);
        assert_eq!(tree[0].children.len(), 2); // foo and bar
    }
}
