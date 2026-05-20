//! Rename implementation for LSP.
//!
//! Handles renaming symbols across the codebase, including validation,
//! prepare-rename info (tsserver-compatible), shorthand property expansion,
//! import specifier handling, and workspace edit generation.
//!
//! Also includes linked editing (JSX tag sync) and file rename support.

mod core;
pub mod file_rename;
pub mod linked_editing;

use rustc_hash::FxHashMap;
use tsz_common::position::{Position, Range};

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// A single text edit.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    /// The range to replace.
    pub range: Range,
    /// The new text.
    pub new_text: String,
}

impl TextEdit {
    /// Create a new text edit.
    pub const fn new(range: Range, new_text: String) -> Self {
        Self { range, new_text }
    }
}

/// A rich text edit used only for rename operations. Includes optional
/// `prefix_text` and `suffix_text` metadata matching tsserver's rename
/// response format. These fields tell the client that the replacement
/// involves a structural expansion (shorthand property, import alias, etc.).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameTextEdit {
    /// The range to replace.
    pub range: Range,
    /// The new text for the identifier.
    pub new_text: String,
    /// Optional prefix text (e.g. `"oldName: "` for shorthand property
    /// expansion `{ x }` -> `{ x: y }`). Matches tsserver's `prefixText`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix_text: Option<String>,
    /// Optional suffix text (e.g. `" as oldName"` for export specifier
    /// expansion `export { x }` -> `export { y as x }`).
    /// Matches tsserver's `suffixText`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix_text: Option<String>,
}

impl RenameTextEdit {
    /// Create a plain rename edit (no prefix/suffix).
    pub const fn new(range: Range, new_text: String) -> Self {
        Self {
            range,
            new_text,
            prefix_text: None,
            suffix_text: None,
        }
    }

    /// Create a rename edit with prefix text.
    pub const fn with_prefix(range: Range, new_text: String, prefix_text: String) -> Self {
        Self {
            range,
            new_text,
            prefix_text: Some(prefix_text),
            suffix_text: None,
        }
    }

    /// Create a rename edit with suffix text.
    pub const fn with_suffix(range: Range, new_text: String, suffix_text: String) -> Self {
        Self {
            range,
            new_text,
            prefix_text: None,
            suffix_text: Some(suffix_text),
        }
    }

    /// Convert to a plain `TextEdit` by folding prefix/suffix into `new_text`.
    pub fn to_text_edit(&self) -> TextEdit {
        let mut text = String::new();
        if let Some(ref prefix) = self.prefix_text {
            text.push_str(prefix);
        }
        text.push_str(&self.new_text);
        if let Some(ref suffix) = self.suffix_text {
            text.push_str(suffix);
        }
        TextEdit::new(self.range, text)
    }
}

/// A workspace edit (changes across multiple files).
#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkspaceEdit {
    /// Map of file path -> list of edits.
    pub changes: FxHashMap<String, Vec<TextEdit>>,
}

impl WorkspaceEdit {
    /// Create a new workspace edit.
    pub fn new() -> Self {
        Self {
            changes: FxHashMap::default(),
        }
    }

    /// Add an edit to the workspace edit.
    pub fn add_edit(&mut self, file_path: String, edit: TextEdit) {
        self.changes.entry(file_path).or_default().push(edit);
    }
}

impl Default for WorkspaceEdit {
    fn default() -> Self {
        Self::new()
    }
}

/// A rename-specific workspace edit that preserves prefix/suffix metadata.
/// Use `to_workspace_edit()` to convert to a standard `WorkspaceEdit`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RenameWorkspaceEdit {
    /// Map of file path -> list of rich rename edits.
    pub changes: FxHashMap<String, Vec<RenameTextEdit>>,
}

impl RenameWorkspaceEdit {
    pub fn new() -> Self {
        Self {
            changes: FxHashMap::default(),
        }
    }

    pub fn add_edit(&mut self, file_path: String, edit: RenameTextEdit) {
        self.changes.entry(file_path).or_default().push(edit);
    }

    /// Convert to a standard `WorkspaceEdit` by folding prefix/suffix into
    /// each edit's `new_text`.
    pub fn to_workspace_edit(&self) -> WorkspaceEdit {
        let mut ws = WorkspaceEdit::new();
        for (file, edits) in &self.changes {
            for edit in edits {
                ws.add_edit(file.clone(), edit.to_text_edit());
            }
        }
        ws
    }
}

impl Default for RenameWorkspaceEdit {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Prepare-rename result (tsserver-compatible)
// ---------------------------------------------------------------------------

/// The kind of a symbol for rename purposes (matches tsserver `ScriptElementKind`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum RenameSymbolKind {
    #[serde(rename = "let")]
    Let,
    #[serde(rename = "const")]
    Const,
    #[serde(rename = "var")]
    Var,
    #[serde(rename = "parameter")]
    Parameter,
    #[serde(rename = "function")]
    Function,
    #[serde(rename = "method")]
    Method,
    #[serde(rename = "property")]
    Property,
    #[serde(rename = "class")]
    Class,
    #[serde(rename = "interface")]
    Interface,
    #[serde(rename = "type")]
    TypeAlias,
    #[serde(rename = "enum")]
    Enum,
    #[serde(rename = "enum member")]
    EnumMember,
    #[serde(rename = "module")]
    Module,
    #[serde(rename = "alias")]
    Alias,
    #[serde(rename = "type parameter")]
    TypeParameter,
    #[serde(rename = "unknown")]
    Unknown,
}

/// Result of `prepare_rename`, providing tsserver-compatible information.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareRenameResult {
    /// Whether this element can be renamed.
    pub can_rename: bool,
    /// Short display name of the symbol (e.g. `"bar"`).
    pub display_name: String,
    /// Qualified display name (e.g. `"Foo.bar"` for a class member).
    pub full_display_name: String,
    /// Symbol kind (matches tsserver `ScriptElementKind`).
    pub kind: RenameSymbolKind,
    /// Comma-separated modifier keywords (e.g. `"export,declare"`).
    pub kind_modifiers: String,
    /// The range of the identifier that triggered the rename request.
    pub trigger_span: Range,
    /// If the rename is not possible, a localized error message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub localized_error_message: Option<String>,
}

impl PrepareRenameResult {
    /// Create a result for when renaming is not allowed.
    pub(crate) fn cannot_rename(msg: &str) -> Self {
        Self {
            can_rename: false,
            display_name: String::new(),
            full_display_name: String::new(),
            kind: RenameSymbolKind::Unknown,
            kind_modifiers: String::new(),
            trigger_span: Range::new(Position::new(0, 0), Position::new(0, 0)),
            localized_error_message: Some(msg.to_string()),
        }
    }
}

// ---------------------------------------------------------------------------
// RenameProvider
// ---------------------------------------------------------------------------

define_lsp_provider!(binder RenameProvider, "Rename provider.");

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
#[path = "../../tests/rename_tests.rs"]
mod rename_tests;
