//! Inlay Hints for the LSP.
//!
//! Inlay hints are inline annotations that show helpful information directly in the source code.
//!
//! Features:
//! - Parameter name hints for function calls
//! - Type hints for variables with implicit types
//! - Generic parameter hints where inferred

use crate::lsp::position::{LineMap, Position, Range};
use crate::parser::syntax_kind_ext;
use crate::parser::thin_node::{NodeAccess, ThinNodeArena};
use crate::parser::NodeIndex;
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
    pub fn provide_inlay_hints(&self, root: NodeIndex, range: Range) -> Vec<InlayHint> {
        let mut hints = Vec::new();

        // Convert range to byte offsets for filtering
        let range_start = self
            .line_map
            .position_to_offset(range.start, self.source)
            .unwrap_or(0);
        let range_end = self
            .line_map
            .position_to_offset(range.end, self.source)
            .unwrap_or(self.source.len() as u32);

        // Traverse AST and collect hints
        self.collect_hints(root, range_start, range_end, &mut hints);

        hints
    }

    /// Recursively collect inlay hints from the AST.
    fn collect_hints(
        &self,
        node_idx: NodeIndex,
        range_start: u32,
        range_end: u32,
        hints: &mut Vec<InlayHint>,
    ) {
        let Some(node) = self.arena.get(node_idx) else {
            return;
        };

        // Skip nodes outside the requested range
        if node.end < range_start || node.pos > range_end {
            return;
        }

        // Collect parameter name hints for call expressions
        if node.kind == syntax_kind_ext::CALL_EXPRESSION {
            self.collect_parameter_hints(node_idx, hints);
        }

        // Collect type hints for variable declarations without explicit types
        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            self.collect_type_hints(node_idx, hints);
        }

        // Recurse into children
        self.arena
            .visit_children(node_idx, |child_idx| {
                self.collect_hints(child_idx, range_start, range_end, hints);
            });
    }

    /// Collect parameter name hints for a call expression.
    fn collect_parameter_hints(&self, call_idx: NodeIndex, hints: &mut Vec<InlayHint>) {
        let Some(node) = self.arena.get(call_idx) else {
            return;
        };
        let Some(call) = self.arena.get_call_expr(node) else {
            return;
        };

        // Get the function being called and resolve its symbol
        let Some(symbol_id) = self.binder.resolve_identifier(self.arena, call.expression) else {
            return;
        };
        let Some(symbol) = self.binder.symbols.get(symbol_id) else {
            return;
        };

        // Get the function declaration to extract parameter names
        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else {
            symbol.declarations.first().copied().unwrap_or(NodeIndex::NONE)
        };

        if decl_idx.is_none() {
            return;
        }

        let param_names = self.get_parameter_names(decl_idx);

        // Match arguments to parameters
        if let Some(args) = &call.arguments {
            for (i, &arg_idx) in args.nodes.iter().enumerate() {
                if i >= param_names.len() {
                    break;
                }
                if let Some(param_name) = &param_names[i] {
                    // Skip if argument is already a named literal or identifier with same name
                    if self.should_skip_parameter_hint(arg_idx, param_name) {
                        continue;
                    }

                    if let Some(arg_node) = self.arena.get(arg_idx) {
                        let pos = self.line_map.offset_to_position(arg_node.pos, self.source);
                        hints.push(InlayHint::new(
                            pos,
                            format!("{}: ", param_name),
                            InlayHintKind::Parameter,
                        ));
                    }
                }
            }
        }
    }

    /// Get parameter names from a function declaration.
    fn get_parameter_names(&self, decl_idx: NodeIndex) -> Vec<Option<String>> {
        let Some(node) = self.arena.get(decl_idx) else {
            return Vec::new();
        };

        let params = if let Some(func) = self.arena.get_function(node) {
            func.parameters.as_ref()
        } else if let Some(arrow) = self.arena.get_arrow_func(node) {
            arrow.parameters.as_ref()
        } else if let Some(method) = self.arena.get_method_decl(node) {
            method.parameters.as_ref()
        } else {
            return Vec::new();
        };

        let Some(params) = params else {
            return Vec::new();
        };

        params
            .nodes
            .iter()
            .map(|&param_idx| {
                let Some(param_node) = self.arena.get(param_idx) else {
                    return None;
                };
                let Some(param) = self.arena.get_parameter(param_node) else {
                    return None;
                };
                self.arena
                    .get_identifier_text(param.name)
                    .map(|s| s.to_string())
            })
            .collect()
    }

    /// Check if we should skip showing a parameter hint for this argument.
    fn should_skip_parameter_hint(&self, arg_idx: NodeIndex, param_name: &str) -> bool {
        let Some(arg_node) = self.arena.get(arg_idx) else {
            return false;
        };

        // Skip if the argument is an identifier with the same name as the parameter
        if arg_node.kind == syntax_kind_ext::IDENTIFIER {
            if let Some(text) = self.arena.get_identifier_text(arg_idx) {
                if text == param_name {
                    return true;
                }
            }
        }

        false
    }

    /// Collect type hints for variable declarations without explicit type annotations.
    fn collect_type_hints(&self, _decl_idx: NodeIndex, _hints: &mut Vec<InlayHint>) {
        // Type hints require type inference which needs the TypeInterner.
        // For now, this is a placeholder. Full implementation would:
        // 1. Check if the variable declaration has no type annotation
        // 2. Get the inferred type from the checker
        // 3. Add a hint showing the inferred type
        //
        // This requires access to the TypeInterner and ThinCheckerState,
        // which would need to be added to InlayHintsProvider.
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
