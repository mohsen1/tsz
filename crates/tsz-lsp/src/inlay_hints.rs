//! Inlay Hints for the LSP.
//!
//! Inlay hints are inline annotations that show helpful information directly in the source code.
//!
//! Features:
//! - Parameter name hints for function calls
//! - Type hints for variables with implicit types (e.g., `let x = 1` shows `: number`)
//! - Return type hints for arrow functions and function expressions
//!
//! ## Type Hints
//!
//! Type hints require integration with the type checker (`CheckerState`) and
//! type storage (`TypeInterner`). The checker infers types from initializer
//! expressions, and the formatter produces human-readable type strings.
//!
//! Type hints are skipped when:
//! - The variable already has an explicit type annotation
//! - There is no initializer expression
//! - The inferred type is `any`, `unknown`, or `error`

use serde::{Deserialize, Serialize};
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_common::position::{LineMap, Position, Range};
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::{NodeAccess, NodeArena};
use tsz_parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeInterner;
use tsz_solver::types::TypeId;

/// Kind of inlay hint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InlayHintKind {
    /// Parameter name hint (e.g., `fn(arg)` -> `fn(arg: paramName)`)
    #[serde(rename = "parameter")]
    Parameter,
    /// Type hint (e.g., `let x = 1` -> `let x: number = 1`)
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
    pub const fn new(position: Position, label: String, kind: InlayHintKind) -> Self {
        Self {
            position,
            label,
            kind,
            tooltip: None,
        }
    }

    /// Create a parameter name hint.
    pub fn parameter(position: Position, param_name: String) -> Self {
        Self::new(
            position,
            format!(": {param_name}"),
            InlayHintKind::Parameter,
        )
    }

    /// Create a type hint.
    pub fn type_hint(position: Position, type_name: String) -> Self {
        Self::new(position, format!(": {type_name}"), InlayHintKind::Type)
    }

    /// Convert to LSP range (for compatibility with other LSP features).
    pub const fn to_range(&self) -> Range {
        Range::new(self.position, self.position)
    }
}

/// Provider for inlay hints.
pub struct InlayHintsProvider<'a> {
    /// The AST node arena.
    pub arena: &'a NodeArena,
    /// The binder state with symbols.
    pub binder: &'a BinderState,
    /// Line map for position calculations.
    pub line_map: &'a LineMap,
    /// Source file text.
    pub source: &'a str,
    /// Type interner for type checking integration.
    pub interner: &'a TypeInterner,
    /// File name for checker context.
    pub file_name: String,
}

impl<'a> InlayHintsProvider<'a> {
    /// Create a new inlay hints provider with type checking support.
    pub const fn new(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        line_map: &'a LineMap,
        source: &'a str,
        interner: &'a TypeInterner,
        file_name: String,
    ) -> Self {
        InlayHintsProvider {
            arena,
            binder,
            line_map,
            source,
            interner,
            file_name,
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

        // Initialize CheckerState for type inference
        let options = CheckerOptions::default();
        let mut checker = CheckerState::new(
            self.arena,
            self.binder,
            self.interner,
            self.file_name.clone(),
            options,
        );

        // Traverse AST and collect hints
        self.collect_hints(root, range_start, range_end, &mut hints, &mut checker);

        hints
    }

    /// Recursively collect inlay hints from the AST.
    fn collect_hints(
        &self,
        node_idx: NodeIndex,
        range_start: u32,
        range_end: u32,
        hints: &mut Vec<InlayHint>,
        checker: &mut CheckerState,
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
            self.collect_type_hints(node_idx, hints, checker);
        }

        // Collect return type hints for arrow functions and function expressions
        if node.kind == syntax_kind_ext::ARROW_FUNCTION
            || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
        {
            self.collect_return_type_hints(node_idx, hints, checker);
        }

        // Recurse into children
        for child_idx in self.arena.get_children(node_idx) {
            self.collect_hints(child_idx, range_start, range_end, hints, checker);
        }
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
        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol
                .declarations
                .first()
                .copied()
                .unwrap_or(NodeIndex::NONE)
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
                            format!("{param_name}: "),
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

        // get_function handles FunctionDeclaration, FunctionExpression, and ArrowFunction
        let params = if let Some(func) = self.arena.get_function(node) {
            Some(&func.parameters)
        } else if let Some(method) = self.arena.get_method_decl(node) {
            Some(&method.parameters)
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
                let param_node = self.arena.get(param_idx)?;
                let param = self.arena.get_parameter(param_node)?;
                self.arena
                    .get_identifier_text(param.name)
                    .map(std::string::ToString::to_string)
            })
            .collect()
    }

    /// Check if we should skip showing a parameter hint for this argument.
    fn should_skip_parameter_hint(&self, arg_idx: NodeIndex, param_name: &str) -> bool {
        let Some(arg_node) = self.arena.get(arg_idx) else {
            return false;
        };

        // Skip if the argument is an identifier with the same name as the parameter
        if arg_node.kind == SyntaxKind::Identifier as u16
            && let Some(text) = self.arena.get_identifier_text(arg_idx)
            && text == param_name
        {
            return true;
        }

        false
    }

    /// Collect type hints for variable declarations without explicit type annotations.
    ///
    /// This examines each `VariableDeclaration` node and, when it has an initializer
    /// but no explicit type annotation, uses the checker to infer the type and create
    /// an inlay hint showing `: type` after the variable name.
    fn collect_type_hints(
        &self,
        decl_idx: NodeIndex,
        hints: &mut Vec<InlayHint>,
        checker: &mut CheckerState,
    ) {
        let Some(node) = self.arena.get(decl_idx) else {
            return;
        };
        let Some(decl) = self.arena.get_variable_declaration(node) else {
            return;
        };

        // Skip if the variable already has an explicit type annotation
        if decl.type_annotation.is_some() {
            return;
        }

        // Skip if there is no initializer (cannot infer type)
        if decl.initializer.is_none() {
            return;
        }

        // Get the inferred type of the declaration node
        let type_id = checker.get_type_of_node(decl_idx);

        // Filter out unhelpful types: error, any, unknown
        if type_id == TypeId::ERROR || type_id == TypeId::ANY || type_id == TypeId::UNKNOWN {
            return;
        }

        // Format the type to a string
        let type_text = checker.format_type(type_id);

        // Additional string-based filter for "any", "unknown", and "error"
        if type_text == "any" || type_text == "unknown" || type_text == "error" {
            return;
        }

        // Position the hint after the variable name identifier
        let Some(name_node) = self.arena.get(decl.name) else {
            return;
        };

        let pos = self.line_map.offset_to_position(name_node.end, self.source);

        hints.push(InlayHint::new(
            pos,
            format!(": {type_text}"),
            InlayHintKind::Type,
        ));
    }

    /// Collect return type hints for arrow functions and function expressions
    /// that lack explicit return type annotations.
    fn collect_return_type_hints(
        &self,
        func_idx: NodeIndex,
        hints: &mut Vec<InlayHint>,
        checker: &mut CheckerState,
    ) {
        let Some(node) = self.arena.get(func_idx) else {
            return;
        };
        let Some(func) = self.arena.get_function(node) else {
            return;
        };

        // Skip if the function already has an explicit return type annotation
        if func.type_annotation.is_some() {
            return;
        }

        // Get the inferred type of the function node (this gives us the full function type)
        let type_id = checker.get_type_of_node(func_idx);

        // Filter out unhelpful types
        if type_id == TypeId::ERROR || type_id == TypeId::ANY || type_id == TypeId::UNKNOWN {
            return;
        }

        let type_text = checker.format_type(type_id);

        // The checker returns the full function type, e.g. "(x: number) => number".
        // We want only the return type portion after "=> ".
        let return_type = if let Some(arrow_pos) = type_text.find("=> ") {
            &type_text[arrow_pos + 3..]
        } else {
            // If we cannot extract a return type from the formatted string, skip
            return;
        };

        // Filter out unhelpful return types
        if return_type == "any" || return_type == "unknown" || return_type == "void" {
            return;
        }

        // Position the hint after the closing paren of the parameter list.
        // Use the end of the last parameter, or if no params, use the body position.
        let hint_offset = if let Some(&last_param) = func.parameters.nodes.last() {
            if let Some(last_node) = self.arena.get(last_param) {
                last_node.end
            } else {
                return;
            }
        } else if let Some(body_node) = self.arena.get(func.body) {
            body_node.pos
        } else {
            return;
        };

        let pos = self.line_map.offset_to_position(hint_offset, self.source);

        hints.push(InlayHint::new(
            pos,
            format!(": {return_type}"),
            InlayHintKind::Type,
        ));
    }
}

#[cfg(test)]
#[path = "../tests/inlay_hints_tests.rs"]
mod inlay_hints_tests;
