//! LSP Code Lens implementation.
//!
//! Code Lens displays actionable information above code elements, such as:
//! - Reference counts: "3 references" above function declarations
//! - Test runners: "Run Test | Debug Test" above test functions
//! - Implementation counts: "2 implementations" above interfaces
//!
//! Code Lenses are displayed inline in the editor and can be clicked to
//! perform actions like "Show References" or "Run Test".

use crate::references::FindReferences;
use tsz_common::position::{Position, Range};
use tsz_parser::{NodeIndex, syntax_kind_ext};

/// A code lens represents a command that can be shown inline with source code.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CodeLens {
    /// The range at which this code lens is displayed.
    pub range: Range,
    /// The command this code lens represents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<CodeLensCommand>,
    /// Additional data that uniquely identifies this code lens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<CodeLensData>,
}

/// A command that can be executed when a code lens is clicked.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CodeLensCommand {
    /// The title of the command (displayed to the user).
    pub title: String,
    /// The identifier of the command to execute.
    pub command: String,
    /// Arguments to pass to the command.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<serde_json::Value>>,
}

/// Additional data for code lens resolution.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CodeLensData {
    /// The file path.
    pub file_path: String,
    /// The kind of code lens.
    pub kind: CodeLensKind,
    /// The position of the symbol.
    pub position: Position,
}

/// The kind of code lens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CodeLensKind {
    /// Show references to this symbol.
    References,
    /// Show implementations of this interface/abstract class.
    Implementations,
    /// Run this test.
    RunTest,
    /// Debug this test.
    DebugTest,
}

impl CodeLens {
    /// Create a new code lens without a command (requires resolution).
    pub fn new(range: Range, data: CodeLensData) -> Self {
        Self {
            range,
            command: None,
            data: Some(data),
        }
    }

    /// Create a resolved code lens with a command.
    pub fn resolved(range: Range, command: CodeLensCommand) -> Self {
        Self {
            range,
            command: Some(command),
            data: None,
        }
    }
}

define_lsp_provider!(binder CodeLensProvider, "Provider for code lenses.");

impl<'a> CodeLensProvider<'a> {
    /// Get all code lenses for the document.
    ///
    /// Returns unresolved code lenses that can be resolved later.
    /// Typically, code lenses are returned quickly without computing
    /// expensive data (like reference counts), and then resolved
    /// when they become visible.
    pub fn provide_code_lenses(&self, _root: NodeIndex) -> Vec<CodeLens> {
        let mut lenses = Vec::new();

        // Collect code lenses for various declaration types
        for (i, node) in self.arena.nodes.iter().enumerate() {
            let node_idx = NodeIndex(i as u32);

            match node.kind {
                // Functions
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                    if let Some(lens) = self.create_reference_lens(node_idx) {
                        lenses.push(lens);
                    }
                }

                // Methods
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(lens) = self.create_reference_lens(node_idx) {
                        lenses.push(lens);
                    }
                }

                // Classes
                k if k == syntax_kind_ext::CLASS_DECLARATION => {
                    if let Some(lens) = self.create_reference_lens(node_idx) {
                        lenses.push(lens);
                    }
                }

                // Interfaces - show implementations lens
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                    if let Some(lens) = self.create_reference_lens(node_idx) {
                        lenses.push(lens);
                    }
                    if let Some(lens) = self.create_implementations_lens(node_idx) {
                        lenses.push(lens);
                    }
                }

                // Type aliases
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                    if let Some(lens) = self.create_reference_lens(node_idx) {
                        lenses.push(lens);
                    }
                }

                // Enums
                k if k == syntax_kind_ext::ENUM_DECLARATION => {
                    if let Some(lens) = self.create_reference_lens(node_idx) {
                        lenses.push(lens);
                    }
                }

                _ => {}
            }
        }

        lenses
    }

    /// Resolve a code lens by computing its command.
    ///
    /// This is called when a code lens becomes visible to compute
    /// expensive data like reference counts.
    pub fn resolve_code_lens(&self, root: NodeIndex, lens: &CodeLens) -> Option<CodeLens> {
        let data = lens.data.as_ref()?;

        match data.kind {
            CodeLensKind::References => self.resolve_references_lens(root, lens, data),
            CodeLensKind::Implementations => self.resolve_implementations_lens(lens, data),
            CodeLensKind::RunTest | CodeLensKind::DebugTest => {
                // Test lenses are typically resolved immediately
                Some(lens.clone())
            }
        }
    }

    /// Create a "references" code lens for a declaration.
    fn create_reference_lens(&self, decl_idx: NodeIndex) -> Option<CodeLens> {
        let node = self.arena.get(decl_idx)?;
        let start = self.line_map.offset_to_position(node.pos, self.source_text);

        // Place the lens at the start of the declaration
        let range = Range::new(start, Position::new(start.line, start.character + 1));

        let data = CodeLensData {
            file_path: self.file_name.clone(),
            kind: CodeLensKind::References,
            position: start,
        };

        Some(CodeLens::new(range, data))
    }

    /// Create an "implementations" code lens for an interface.
    fn create_implementations_lens(&self, decl_idx: NodeIndex) -> Option<CodeLens> {
        let node = self.arena.get(decl_idx)?;
        let start = self.line_map.offset_to_position(node.pos, self.source_text);

        let range = Range::new(start, Position::new(start.line, start.character + 1));

        let data = CodeLensData {
            file_path: self.file_name.clone(),
            kind: CodeLensKind::Implementations,
            position: start,
        };

        Some(CodeLens::new(range, data))
    }

    /// Resolve a references lens by counting references.
    fn resolve_references_lens(
        &self,
        root: NodeIndex,
        lens: &CodeLens,
        data: &CodeLensData,
    ) -> Option<CodeLens> {
        // Find references using the existing FindReferences implementation
        let finder = FindReferences::new(
            self.arena,
            self.binder,
            self.line_map,
            self.file_name.clone(),
            self.source_text,
        );

        let references = finder.find_references(root, data.position);
        let count = references.as_ref().map_or(0, |r| r.len());

        // Subtract 1 for the declaration itself (if it's included in references)
        let ref_count = if count > 0 { count - 1 } else { 0 };

        let title = if ref_count == 1 {
            "1 reference".to_string()
        } else {
            format!("{} references", ref_count)
        };

        let command = CodeLensCommand {
            title,
            command: "editor.action.showReferences".to_string(),
            arguments: Some(vec![
                serde_json::json!(data.file_path),
                serde_json::json!({
                    "line": data.position.line,
                    "character": data.position.character
                }),
                serde_json::json!([]),
            ]),
        };

        Some(CodeLens::resolved(lens.range, command))
    }

    /// Resolve an implementations lens.
    fn resolve_implementations_lens(
        &self,
        lens: &CodeLens,
        data: &CodeLensData,
    ) -> Option<CodeLens> {
        // Finding implementations requires type checking and class hierarchy analysis
        // For now, return a placeholder that shows the command is available
        let command = CodeLensCommand {
            title: "Find Implementations".to_string(),
            command: "editor.action.goToImplementation".to_string(),
            arguments: Some(vec![
                serde_json::json!(data.file_path),
                serde_json::json!({
                    "line": data.position.line,
                    "character": data.position.character
                }),
            ]),
        };

        Some(CodeLens::resolved(lens.range, command))
    }
}

#[cfg(test)]
#[path = "../tests/code_lens_tests.rs"]
mod code_lens_tests;
