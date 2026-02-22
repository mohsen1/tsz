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
//! - Workspace Symbols
//!
//! Architecture:
//! - Position utilities for line/column <-> offset conversion
//! - AST node lookup by position
//! - Symbol-based navigation using binder data

#[macro_use]
pub mod provider_macro;
pub mod code_actions;
pub mod code_lens;
pub mod completions;
pub mod dependency_graph;
pub mod diagnostics;
pub mod document_links;
pub mod export_signature;
pub mod file_rename;
pub mod folding;
pub mod formatting;
pub mod hierarchy;
pub mod highlighting;
pub mod hover;
pub mod inlay_hints;
pub mod jsdoc;
pub mod linked_editing;
pub use tsz_common::position;
pub mod navigation;
pub mod project;
pub mod rename;
pub mod resolver;
pub mod selection_range;
pub mod semantic_tokens;
pub mod signature_help;
pub mod symbols;
pub mod utils;

#[cfg(test)]
#[path = "../tests/code_actions_tests.rs"]
mod code_actions_tests;
#[cfg(test)]
#[path = "../tests/project_tests.rs"]
mod project_tests;
#[cfg(test)]
#[path = "../tests/tests.rs"]
mod tests;

pub use code_actions::{
    CodeAction, CodeActionContext, CodeActionKind, CodeActionProvider, CodeFixRegistry,
    ImportCandidate, ImportCandidateKind,
};
pub use completions::{CompletionItem, CompletionItemKind, Completions};
pub use diagnostics::{DiagnosticSeverity, LspDiagnostic};
pub use folding::{FoldingRange, FoldingRangeProvider};
pub use formatting::{
    DocumentFormattingProvider, FormattingOptions, TextEdit as FormattingTextEdit,
};
pub use highlighting::{DocumentHighlight, DocumentHighlightKind, DocumentHighlightProvider};
pub use hover::{HoverInfo, HoverProvider};
pub use navigation::definition::GoToDefinition;
pub use navigation::references::{FindReferences, ReferenceInfo, RenameLocation};
pub use navigation::{definition, implementation, references, type_definition};
pub use position::{Location, Position, Range, SourceLocation};
pub use project::{
    Project, ProjectFile, ProjectPerformance, ProjectRequestKind, ProjectRequestTiming,
};
pub use rename::{RenameProvider, TextEdit, WorkspaceEdit};
pub use semantic_tokens::{SemanticTokenType, SemanticTokensProvider, semantic_token_modifiers};
pub use signature_help::{
    ParameterInformation, SignatureHelp, SignatureHelpProvider, SignatureInformation,
};
pub use symbols::DocumentSymbols;
pub use symbols::{DocumentSymbol, DocumentSymbolProvider, SymbolKind};

// Selection Range
pub use selection_range::{SelectionRange, SelectionRangeProvider};

// Type Definition
pub use navigation::type_definition::TypeDefinitionProvider;

// Code Lens
pub use code_lens::{CodeLens, CodeLensCommand, CodeLensData, CodeLensKind, CodeLensProvider};

// Symbol Index
pub use symbols::SymbolIndex;

// Document Links
pub use document_links::{DocumentLink, DocumentLinkProvider};

// Workspace Symbols
pub use symbols::{SymbolInformation, WorkspaceSymbolsProvider};

// Go to Implementation
pub use navigation::implementation::GoToImplementationProvider;

// Call Hierarchy
pub use hierarchy::call_hierarchy::{
    CallHierarchyIncomingCall, CallHierarchyItem, CallHierarchyOutgoingCall, CallHierarchyProvider,
};

// Type Hierarchy
pub use hierarchy::type_hierarchy::{TypeHierarchyItem, TypeHierarchyProvider};

// Linked Editing
pub use linked_editing::{LinkedEditingProvider, LinkedEditingRanges};

// File Rename
pub use file_rename::{FileRenameProvider, ImportLocation};
