//! Inlay Hints for the LSP.
//!
//! Inlay hints are inline annotations that show helpful information directly in the source code.
//!
//! Features:
//! - Parameter name hints for function calls
//! - Type hints for variables with implicit types
//! - Generic parameter hints where inferred

use crate::lsp::position::{LineMap, Position, Range};
use crate::parser::NodeIndex;
use crate::parser::thin_node::ThinNodeArena;
use crate::thin_binder::ThinBinderState;
use serde::{Deserialize, Serialize};

/// Kind of inlay hint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InlayHintKind {
    /// Parameter name hint (e.g., `fn(arg)` → `fn(arg: paramName)`)
    #[serde(rename = "parameter")]
    Parameter,
    /// Type hint (e.g., `let x = 1` → `let x: number = 1`)
    #[serde(rename = "type")]
    Type,
    /// Generic parameter hint
    #[serde(rename = "generic")]
    Generic,
}

/// An inlay hint - an inline annotation in the source code.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InlayHint {
    /// Position where the hint should be shown.
    pub position: Position,
    /// The label to show (e.g., ": paramName" or ": number").
    pub label: String,
    /// Kind of hint.
    pub kind: InlayHintKind,
    /// Optional tooltip with additional information.
    pub tooltip: Option<String>,
}

impl InlayHint {
    /// Create a new inlay hint.
    pub fn new(position: Position, label: String, kind: InlayHintKind) -> Self {
        InlayHint {
            position,
            label,
            kind,
            tooltip: None,
        }
    }

    /// Create a parameter name hint.
    pub fn parameter(position: Position, param_name: String) -> Self {
        InlayHint::new(
            position,
            format!(": {}", param_name),
            InlayHintKind::Parameter,
        )
    }

    /// Create a type hint.
    pub fn type_hint(position: Position, type_name: String) -> Self {
        InlayHint::new(
            position,
            format!(": {}", type_name),
            InlayHintKind::Type,
        )
    }

    /// Convert to LSP range (for compatibility with other LSP features).
    pub fn to_range(&self) -> Range {
        Range::new(self.position, self.position)
    }
}

/// Provider for inlay hints.
pub struct InlayHintsProvider<'a> {
    /// The AST node arena.
    pub arena: &'a ThinNodeArena,
    /// The binder state with symbols.
    pub binder: &'a ThinBinderState,
    /// Line map for position calculations.
    pub line_map: &'a LineMap,
    /// Source file text.
    pub source: &'a str,
}

impl<'a> InlayHintsProvider<'a> {
    /// Create a new inlay hints provider.
    pub fn new(
        arena: &'a ThinNodeArena,
        binder: &'a ThinBinderState,
        line_map: &'a LineMap,
        source: &'a str,
    ) -> Self {
        InlayHintsProvider {
            arena,
            binder,
            line_map,
            source,
        }
    }

    /// Provide inlay hints for the given range.
    pub fn provide_inlay_hints(&self, _root: NodeIndex, _range: Range) -> Vec<InlayHint> {
        let hints = Vec::new();

        // For now, return empty hints
        // Full implementation will traverse the AST and collect hints
        // TODO: Implement parameter name hints
        // TODO: Implement type hints

        hints
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inlay_hint_parameter() {
        let position = Position::new(0, 10);
        let hint = InlayHint::parameter(position, "paramName".to_string());

        assert_eq!(hint.position, position);
        assert_eq!(hint.label, ": paramName");
        assert_eq!(hint.kind, InlayHintKind::Parameter);
    }

    #[test]
    fn test_inlay_hint_type() {
        let position = Position::new(0, 10);
        let hint = InlayHint::type_hint(position, "number".to_string());

        assert_eq!(hint.position, position);
        assert_eq!(hint.label, ": number");
        assert_eq!(hint.kind, InlayHintKind::Type);
    }
}
