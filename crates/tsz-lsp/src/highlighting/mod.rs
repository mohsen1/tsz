//! Highlighting providers for LSP.
//!
//! - `document`: Document highlighting (highlight all occurrences of a symbol)
//! - `semantic_tokens`: Semantic tokens (rich syntax coloring based on symbol types)

pub mod document;
pub mod semantic_tokens;

pub use document::{DocumentHighlight, DocumentHighlightKind, DocumentHighlightProvider};
pub use semantic_tokens::{SemanticTokenType, SemanticTokensProvider, semantic_token_modifiers};
