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
/// This provides a simplified interface to the `DocumentSymbolProvider`.
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
#[path = "../tests/symbols_tests.rs"]
mod symbols_tests;
