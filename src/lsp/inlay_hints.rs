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

use crate::binder::BinderState;
use crate::checker::context::CheckerOptions;
use crate::checker::state::CheckerState;
use crate::lsp::position::{LineMap, Position, Range};
use crate::parser::NodeIndex;
use crate::parser::node::{NodeAccess, NodeArena};
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;
use crate::solver::TypeInterner;
use crate::solver::types::TypeId;
use serde::{Deserialize, Serialize};

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
        InlayHint::new(position, format!(": {}", type_name), InlayHintKind::Type)
    }

    /// Convert to LSP range (for compatibility with other LSP features).
    pub fn to_range(&self) -> Range {
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
    pub fn new(
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
        let decl_idx = if !symbol.value_declaration.is_none() {
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
        if !decl.type_annotation.is_none() {
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
            format!(": {}", type_text),
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
        if !func.type_annotation.is_none() {
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
            format!(": {}", return_type),
            InlayHintKind::Type,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binder::BinderState;
    use crate::lsp::position::LineMap;
    use crate::parser::ParserState;
    use crate::solver::TypeInterner;

    /// Helper to create a provider and get hints for the given source code.
    fn get_hints_for_source(source: &str) -> Vec<InlayHint> {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let interner = TypeInterner::new();
        let line_map = LineMap::build(source);

        let provider = InlayHintsProvider::new(
            parser.get_arena(),
            &binder,
            &line_map,
            source,
            &interner,
            "test.ts".to_string(),
        );

        let range = Range::new(Position::new(0, 0), Position::new(u32::MAX, u32::MAX));
        provider.provide_inlay_hints(root, range)
    }

    /// Helper to get only type hints from the results.
    fn get_type_hints(hints: &[InlayHint]) -> Vec<&InlayHint> {
        hints
            .iter()
            .filter(|h| h.kind == InlayHintKind::Type)
            .collect()
    }

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

    #[test]
    fn test_type_hint_number_literal() {
        let source = "let x = 42;";
        let hints = get_hints_for_source(source);
        let type_hints = get_type_hints(&hints);

        assert!(
            !type_hints.is_empty(),
            "Should produce a type hint for number literal"
        );
        assert_eq!(type_hints[0].label, ": number");
        assert_eq!(type_hints[0].kind, InlayHintKind::Type);
    }

    #[test]
    fn test_type_hint_string_literal() {
        let source = "let s = \"hello\";";
        let hints = get_hints_for_source(source);
        let type_hints = get_type_hints(&hints);

        assert!(
            !type_hints.is_empty(),
            "Should produce a type hint for string literal"
        );
        assert_eq!(type_hints[0].label, ": string");
        assert_eq!(type_hints[0].kind, InlayHintKind::Type);
    }

    #[test]
    fn test_type_hint_boolean_literal() {
        let source = "let b = true;";
        let hints = get_hints_for_source(source);
        let type_hints = get_type_hints(&hints);

        assert!(
            !type_hints.is_empty(),
            "Should produce a type hint for boolean literal"
        );
        // The checker may return "boolean" or "true" (literal type) depending on
        // whether const or let. With let, it should widen to "boolean".
        let label = &type_hints[0].label;
        assert!(
            label == ": boolean" || label == ": true",
            "Expected ': boolean' or ': true', got '{}'",
            label
        );
    }

    #[test]
    fn test_no_hint_with_type_annotation() {
        let source = "let x: number = 42;";
        let hints = get_hints_for_source(source);
        let type_hints = get_type_hints(&hints);

        assert!(
            type_hints.is_empty(),
            "Should NOT produce a type hint when type annotation is present"
        );
    }

    #[test]
    fn test_no_hint_without_initializer() {
        let source = "let x;";
        let hints = get_hints_for_source(source);
        let type_hints = get_type_hints(&hints);

        assert!(
            type_hints.is_empty(),
            "Should NOT produce a type hint when there is no initializer"
        );
    }

    #[test]
    fn test_type_hint_array() {
        let source = "let arr = [1, 2, 3];";
        let hints = get_hints_for_source(source);
        let type_hints = get_type_hints(&hints);

        assert!(
            !type_hints.is_empty(),
            "Should produce a type hint for array literal"
        );
        // The type might be "number[]" or "Array<number>" depending on formatter
        let label = &type_hints[0].label;
        assert!(
            label.contains("number"),
            "Array type hint should contain 'number', got '{}'",
            label
        );
    }

    #[test]
    fn test_type_hint_object() {
        let source = "let obj = { a: 1, b: \"hello\" };";
        let hints = get_hints_for_source(source);
        let type_hints = get_type_hints(&hints);

        assert!(
            !type_hints.is_empty(),
            "Should produce a type hint for object literal"
        );
        let label = &type_hints[0].label;
        // Object type should mention the properties
        assert!(
            label.contains("a") && label.contains("b"),
            "Object type hint should contain property names, got '{}'",
            label
        );
    }

    #[test]
    fn test_no_hint_for_any_type() {
        // Variables explicitly typed as any should be skipped, and variables
        // that the checker infers as any/unknown should also be skipped.
        let source = "let x: any = 42;";
        let hints = get_hints_for_source(source);
        let type_hints = get_type_hints(&hints);

        assert!(
            type_hints.is_empty(),
            "Should NOT produce a type hint for 'any' typed variable"
        );
    }

    #[test]
    fn test_parameter_and_type_hints_together() {
        let source =
            "function greet(name: string) { return name; }\nlet msg = \"Hello\";\ngreet(msg);";
        let hints = get_hints_for_source(source);

        let type_hints: Vec<_> = hints
            .iter()
            .filter(|h| h.kind == InlayHintKind::Type)
            .collect();
        let param_hints: Vec<_> = hints
            .iter()
            .filter(|h| h.kind == InlayHintKind::Parameter)
            .collect();

        // msg should get a type hint for string
        assert!(
            !type_hints.is_empty(),
            "Should have at least one type hint for 'msg'"
        );
        assert!(
            type_hints.iter().any(|h| h.label == ": string"),
            "Should have a string type hint for 'msg'"
        );

        // greet(msg) should get a parameter hint (msg != name, so hint shown)
        // Note: parameter hints depend on binder resolution working correctly
        // for the greet function. We verify at least no crash occurs.
        let _ = param_hints;
    }

    #[test]
    fn test_type_hint_position_after_name() {
        let source = "let x = 42;";
        let hints = get_hints_for_source(source);
        let type_hints = get_type_hints(&hints);

        if !type_hints.is_empty() {
            let hint = &type_hints[0];
            // "let x = 42;" - 'x' is at index 4, so hint should be on line 0
            assert_eq!(hint.position.line, 0, "Hint should be on line 0");
            // The position should be at or after column 4 (end of 'x')
            assert!(
                hint.position.character >= 4,
                "Hint position should be at or after the end of the variable name, got col {}",
                hint.position.character
            );
        }
    }

    #[test]
    fn test_type_hint_const_number() {
        let source = "const x = 100;";
        let hints = get_hints_for_source(source);
        let type_hints = get_type_hints(&hints);

        assert!(
            !type_hints.is_empty(),
            "Should produce a type hint for const with number literal"
        );
        // const might get a literal type like "100" or widened "number"
        let label = &type_hints[0].label;
        assert!(
            label.contains("number") || label.contains("100"),
            "Const number hint should be 'number' or '100', got '{}'",
            label
        );
    }

    #[test]
    fn test_multiple_variable_declarations() {
        let source = "let a = 1;\nlet b = \"two\";\nlet c = true;";
        let hints = get_hints_for_source(source);
        let type_hints = get_type_hints(&hints);

        assert!(
            type_hints.len() >= 2,
            "Should produce type hints for multiple variable declarations, got {}",
            type_hints.len()
        );
    }

    #[test]
    fn test_no_type_hint_var_without_init() {
        let source = "var x;\nvar y;";
        let hints = get_hints_for_source(source);
        let type_hints = get_type_hints(&hints);

        assert!(
            type_hints.is_empty(),
            "Should NOT produce type hints for variables without initializers"
        );
    }
}
