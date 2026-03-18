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
pub mod completions;
pub mod dependency_graph;
pub mod diagnostics;
pub mod document_links;
pub mod editor_decorations;
pub mod editor_ranges;
pub mod export_signature;
pub mod formatting;
pub mod hierarchy;
pub mod highlighting;
pub mod hover;
pub mod jsdoc;
pub use tsz_common::position;
pub mod navigation;
pub mod project;
pub mod rename;
pub mod resolver;
pub mod signature_help;
pub mod symbols;
pub mod utils;

pub mod fourslash;

#[cfg(test)]
#[path = "../tests/code_actions_tests.rs"]
mod code_actions_tests;
#[cfg(test)]
#[path = "../tests/fourslash_tests.rs"]
mod fourslash_tests;
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
pub use completions::{CompletionItem, CompletionItemData, CompletionItemKind, Completions};
pub use diagnostics::{
    DiagnosticSeverity, DocumentDiagnosticReportKind, FullDocumentDiagnosticReport, LspDiagnostic,
    UnchangedDocumentDiagnosticReport, WorkspaceDiagnosticReport, WorkspaceDiagnosticReportItem,
};
pub use editor_ranges::folding::{FoldingRange, FoldingRangeProvider};
pub use formatting::{
    DocumentFormattingProvider, FormattingOptions, TextEdit as FormattingTextEdit,
};
pub use highlighting::semantic_tokens::{
    SemanticTokenType, SemanticTokensProvider, semantic_token_modifiers,
};
pub use highlighting::{DocumentHighlight, DocumentHighlightKind, DocumentHighlightProvider};
pub use hover::{HoverInfo, HoverProvider};
pub use jsdoc::jsdoc_for_node;
pub use navigation::declaration::GoToDeclarationProvider;
pub use navigation::definition::GoToDefinition;
pub use navigation::references::{FindReferences, ReferenceInfo, RenameLocation};
pub use navigation::source_definition::GoToSourceDefinitionProvider;
pub use navigation::{
    declaration, definition, implementation, references, source_definition, type_definition,
};
pub use position::{Location, Position, Range, SourceLocation};
pub use project::{
    FileRename, Project, ProjectFile, ProjectPerformance, ProjectRequestKind, ProjectRequestTiming,
    TsConfigSettings,
};
pub use rename::{RenameProvider, TextEdit, WorkspaceEdit};
pub use signature_help::{
    ParameterInformation, SignatureHelp, SignatureHelpProvider, SignatureInformation,
};
pub use symbols::DocumentSymbols;
pub use symbols::{DocumentSymbol, DocumentSymbolProvider, SymbolKind};

// Selection Range
pub use editor_ranges::selection_range::{SelectionRange, SelectionRangeProvider};

// Type Definition
pub use navigation::type_definition::TypeDefinitionProvider;

// Code Lens
pub use editor_decorations::code_lens::{
    CodeLens, CodeLensCommand, CodeLensData, CodeLensKind, CodeLensProvider,
};

// Inlay Hints
pub use editor_decorations::inlay_hints::{InlayHint, InlayHintKind, InlayHintsProvider};

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
pub use rename::linked_editing::{LinkedEditingProvider, LinkedEditingRanges};

// File Rename
pub use rename::file_rename::{FileRenameProvider, ImportLocation};

// Document Colors
pub use editor_decorations::document_color::{Color, ColorInformation, DocumentColorProvider};
