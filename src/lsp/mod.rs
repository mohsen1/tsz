//! LSP (Language Server Protocol) support for the WASM TypeScript compiler.
//!
//! This module provides LSP features:
//! - Go to Definition
//! - Go to Type Definition
//! - Find References
//! - Completions
//! - Hover
//! - Signature Help
//! - Document Symbols
//! - Document Formatting
//! - Document Highlighting
//! - Rename
//! - Semantic Tokens
//! - Folding Ranges
//! - Code Lens
//! - Selection Range
//! - Code Actions
//! - Diagnostics
//!
//! Architecture:
//! - Position utilities for line/column <-> offset conversion
//! - AST node lookup by position
//! - Symbol-based navigation using binder data

pub mod code_actions;
pub mod code_lens;
pub mod completions;
pub mod definition;
pub mod diagnostics;
pub mod document_symbols;
pub mod folding;
pub mod formatting;
pub mod highlighting;
pub mod hover;
pub mod inlay_hints;
pub mod jsdoc;
pub mod position;
pub mod project;
pub mod references;
pub mod rename;
pub mod resolver;
pub mod selection_range;
pub mod semantic_tokens;
pub mod signature_help;
pub mod symbols;
pub mod type_definition;
pub mod utils;

#[cfg(test)]
mod code_actions_tests;
#[cfg(test)]
mod project_tests;
#[cfg(test)]
mod tests;

pub use code_actions::{
    CodeAction, CodeActionContext, CodeActionKind, CodeActionProvider, ImportCandidate,
    ImportCandidateKind,
};
pub use completions::{CompletionItem, CompletionItemKind, Completions};
pub use definition::GoToDefinition;
pub use diagnostics::{DiagnosticSeverity, LspDiagnostic};
pub use document_symbols::{DocumentSymbol, DocumentSymbolProvider, SymbolKind};
pub use folding::{FoldingRange, FoldingRangeProvider};
pub use formatting::{
    DocumentFormattingProvider, FormattingOptions, TextEdit as FormattingTextEdit,
};
pub use highlighting::{DocumentHighlight, DocumentHighlightKind, DocumentHighlightProvider};
pub use hover::{HoverInfo, HoverProvider};
pub use position::{Location, Position, Range, SourceLocation};
pub use project::{
    Project, ProjectFile, ProjectPerformance, ProjectRequestKind, ProjectRequestTiming,
};
pub use references::FindReferences;
pub use rename::{RenameProvider, TextEdit, WorkspaceEdit};
pub use semantic_tokens::{SemanticTokenType, SemanticTokensProvider, semantic_token_modifiers};
pub use signature_help::{
    ParameterInformation, SignatureHelp, SignatureHelpProvider, SignatureInformation,
};
pub use symbols::DocumentSymbols;

// Selection Range
pub use selection_range::{SelectionRange, SelectionRangeProvider};

// Type Definition
pub use type_definition::TypeDefinitionProvider;

// Code Lens
pub use code_lens::{CodeLens, CodeLensCommand, CodeLensData, CodeLensKind, CodeLensProvider};
