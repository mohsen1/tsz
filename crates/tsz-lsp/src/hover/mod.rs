//! Hover implementation for LSP.
//!
//! Displays type information and documentation for the symbol at the cursor.
//! Produces quickinfo output compatible with tsserver's expected format:
//! - `display_string`: The raw signature (e.g. `const x: number`, `function foo(): void`)
//! - `kind`: The symbol kind (e.g. `const`, `function`, `class`)
//! - `kind_modifiers`: Comma-separated modifier list (e.g. `export,declare`)
//! - `documentation`: Extracted `JSDoc` content

mod contextual;
mod core;
pub(crate) mod format;

use tsz_common::position::Range;
// Re-export Position for test module (uses `super::*`)
#[cfg(test)]
use tsz_common::position::Position;

/// A single `JSDoc` tag (e.g. `@param`, `@returns`, `@deprecated`).
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct JsDocTag {
    /// The tag name (e.g. "param", "returns", "deprecated")
    pub name: String,
    /// The tag text content
    pub text: String,
}

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
    /// The documentation text extracted from `JSDoc`
    pub documentation: String,
    /// `JSDoc` tags (e.g. @param, @returns, @deprecated)
    pub tags: Vec<JsDocTag>,
}

define_lsp_provider!(full HoverProvider, "Hover provider.");

#[cfg(test)]
#[path = "../../tests/hover_tests.rs"]
mod hover_tests;
