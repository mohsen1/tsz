//! Optional Chaining and Nullish Coalescing ES5 Transform
//!
//! Transforms ES2020 optional chaining (`?.`) and nullish coalescing (`??`)
//! operators to ES5-compatible conditionals.
//!
//! ## Optional Chaining Transform
//!
//! ```typescript
//! // Property access
//! obj?.prop
//! ```
//! Becomes:
//! ```javascript
//! obj === null || obj === void 0 ? void 0 : obj.prop
//! ```
//!
//! ## Nested Optional Chains
//!
//! ```typescript
//! a?.b?.c
//! ```
//! Becomes:
//! ```javascript
//! var _a;
//! (_a = a === null || a === void 0 ? void 0 : a.b) === null || _a === void 0 ? void 0 : _a.c
//! ```
//!
//! ## Optional Call Transform
//!
//! ```typescript
//! func?.()
//! ```
//! Becomes:
//! ```javascript
//! func === null || func === void 0 ? void 0 : func()
//! ```
//!
//! ## Nullish Coalescing Transform
//!
//! ```typescript
//! a ?? b
//! ```
//! Becomes:
//! ```javascript
//! a !== null && a !== void 0 ? a : b
//! ```

use crate::parser::syntax_kind_ext;
use crate::parser::node::NodeArena;
use crate::parser::{NodeIndex, NodeList};
use crate::scanner::SyntaxKind;
use crate::transforms::ir::*;

/// ES5 Optional Chain Transformer - produces IR nodes for optional chaining lowering
pub struct ES5OptionalChainTransformer<'a> {
    arena: &'a NodeArena,
    /// Counter for temporary variable names
    temp_var_counter: u32,
}

impl<'a> ES5OptionalChainTransformer<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        Self {
            arena,
            temp_var_counter: 0,
        }
    }

    /// Reset temporary variable counter
    pub fn reset(&mut self) {
        self.temp_var_counter = 0;
    }

    /// Get next temporary variable name
    fn next_temp_var(&mut self) -> String {
        let name = format!("_{}", (b'a' + (self.temp_var_counter % 26) as u8) as char);
        self.temp_var_counter += 1;
        name
    }

    /// Check if an expression is an optional chain
    pub fn is_optional_chain(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                if let Some(access) = self.arena.get_access_expr(node) {
                    access.question_dot_token
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Check if a binary expression is nullish coalescing
    pub fn is_nullish_coalescing(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }

        let Some(binary) = self.arena.get_binary_expr(node) else {
            return false;
        };

        binary.operator_token == SyntaxKind::QuestionQuestionToken as u16
    }

    /// Transform an optional chain expression to ES5
    ///
    /// `obj?.prop` becomes `obj === null || obj === void 0 ? void 0 : obj.prop`
    pub fn transform_optional_chain(&mut self, idx: NodeIndex) -> Option<IRNode> {
        let node = self.arena.get(idx)?;

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                self.transform_optional_property_access(idx)
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                self.transform_optional_element_access(idx)
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                self.transform_optional_call(idx)
            }
            _ => None,
        }
    }

    /// Transform optional property access: `obj?.prop`
    fn transform_optional_property_access(&mut self, idx: NodeIndex) -> Option<IRNode> {
        let node = self.arena.get(idx)?;
        let access = self.arena.get_access_expr(node)?;

        if !access.question_dot_token {
            // Not optional - just transform normally
            let object = self.transform_expression(access.expression)?;
            let property = self.get_identifier_text(access.name_or_argument);
            return Some(IRNode::prop(object, &property));
        }

        // Optional chain: obj?.prop
        // Transform to: obj === null || obj === void 0 ? void 0 : obj.prop

        let object_expr = self.transform_expression(access.expression)?;
        let property = self.get_identifier_text(access.name_or_argument);

        // For complex expressions, use temp variable to avoid double evaluation
        let needs_temp = self.is_complex_expression(access.expression);

        if needs_temp {
            let temp = self.next_temp_var();
            // (_a = obj, _a === null || _a === void 0 ? void 0 : _a.prop)
            Some(IRNode::CommaExpr(vec![
                IRNode::assign(IRNode::id(&temp), object_expr),
                self.build_nullish_check(
                    IRNode::id(&temp),
                    IRNode::prop(IRNode::id(&temp), &property),
                ),
            ]))
        } else {
            self.build_nullish_check(
                object_expr.clone(),
                IRNode::prop(object_expr, &property),
            )
            .into()
        }
    }

    /// Transform optional element access: `arr?.[index]`
    fn transform_optional_element_access(&mut self, idx: NodeIndex) -> Option<IRNode> {
        let node = self.arena.get(idx)?;
        let access = self.arena.get_access_expr(node)?;

        if !access.question_dot_token {
            // Not optional - just transform normally
            let object = self.transform_expression(access.expression)?;
            let index = self.transform_expression(access.name_or_argument)?;
            return Some(IRNode::elem(object, index));
        }

        // Optional chain: arr?.[index]
        // Transform to: arr === null || arr === void 0 ? void 0 : arr[index]

        let object_expr = self.transform_expression(access.expression)?;
        let index_expr = self.transform_expression(access.name_or_argument)?;

        let needs_temp = self.is_complex_expression(access.expression);

        if needs_temp {
            let temp = self.next_temp_var();
            Some(IRNode::CommaExpr(vec![
                IRNode::assign(IRNode::id(&temp), object_expr),
                self.build_nullish_check(
                    IRNode::id(&temp),
                    IRNode::elem(IRNode::id(&temp), index_expr),
                ),
            ]))
        } else {
            self.build_nullish_check(
                object_expr.clone(),
                IRNode::elem(object_expr, index_expr),
            )
            .into()
        }
    }

    /// Transform optional call: `func?.()`
    fn transform_optional_call(&mut self, idx: NodeIndex) -> Option<IRNode> {
        let node = self.arena.get(idx)?;
        let call = self.arena.get_call_expr(node)?;

        // Check if callee is an optional chain
        if !self.is_optional_chain(call.expression) {
            // Check if the call itself is part of optional chain
            // This handles func?.() pattern
            return None;
        }

        let callee_expr = self.transform_expression(call.expression)?;

        // Transform arguments
        let args = if let Some(ref arg_list) = call.arguments {
            arg_list
                .nodes
                .iter()
                .filter_map(|&arg_idx| self.transform_expression(arg_idx))
                .collect()
        } else {
            vec![]
        };

        let temp = self.next_temp_var();

        // (_a = callee, _a === null || _a === void 0 ? void 0 : _a(...args))
        Some(IRNode::CommaExpr(vec![
            IRNode::assign(IRNode::id(&temp), callee_expr),
            self.build_nullish_check(
                IRNode::id(&temp),
                IRNode::call(IRNode::id(&temp), args),
            ),
        ]))
    }

    /// Transform nullish coalescing: `a ?? b`
    ///
    /// Becomes: `a !== null && a !== void 0 ? a : b`
    pub fn transform_nullish_coalescing(&mut self, idx: NodeIndex) -> Option<IRNode> {
        let node = self.arena.get(idx)?;
        let binary = self.arena.get_binary_expr(node)?;

        if binary.operator_token != SyntaxKind::QuestionQuestionToken as u16 {
            return None;
        }

        let left_expr = self.transform_expression(binary.left)?;
        let right_expr = self.transform_expression(binary.right)?;

        let needs_temp = self.is_complex_expression(binary.left);

        if needs_temp {
            let temp = self.next_temp_var();
            // (_a = left, _a !== null && _a !== void 0 ? _a : right)
            Some(IRNode::CommaExpr(vec![
                IRNode::assign(IRNode::id(&temp), left_expr),
                IRNode::ConditionalExpr {
                    condition: Box::new(IRNode::LogicalAnd {
                        left: Box::new(IRNode::binary(
                            IRNode::id(&temp),
                            "!==",
                            IRNode::NullLiteral,
                        )),
                        right: Box::new(IRNode::binary(
                            IRNode::id(&temp),
                            "!==",
                            IRNode::Undefined,
                        )),
                    }),
                    when_true: Box::new(IRNode::id(&temp)),
                    when_false: Box::new(right_expr),
                },
            ]))
        } else {
            Some(IRNode::ConditionalExpr {
                condition: Box::new(IRNode::LogicalAnd {
                    left: Box::new(IRNode::binary(
                        left_expr.clone(),
                        "!==",
                        IRNode::NullLiteral,
                    )),
                    right: Box::new(IRNode::binary(
                        left_expr.clone(),
                        "!==",
                        IRNode::Undefined,
                    )),
                }),
                when_true: Box::new(left_expr),
                when_false: Box::new(right_expr),
            })
        }
    }

    /// Transform nullish coalescing assignment: `a ??= b`
    ///
    /// Becomes: `a !== null && a !== void 0 ? a : (a = b)`
    pub fn transform_nullish_assignment(&mut self, idx: NodeIndex) -> Option<IRNode> {
        let node = self.arena.get(idx)?;
        let binary = self.arena.get_binary_expr(node)?;

        if binary.operator_token != SyntaxKind::QuestionQuestionEqualsToken as u16 {
            return None;
        }

        let target_expr = self.transform_expression(binary.left)?;
        let value_expr = self.transform_expression(binary.right)?;

        // target ??= value becomes:
        // target !== null && target !== void 0 ? target : (target = value)
        Some(IRNode::ConditionalExpr {
            condition: Box::new(IRNode::LogicalAnd {
                left: Box::new(IRNode::binary(
                    target_expr.clone(),
                    "!==",
                    IRNode::NullLiteral,
                )),
                right: Box::new(IRNode::binary(
                    target_expr.clone(),
                    "!==",
                    IRNode::Undefined,
                )),
            }),
            when_true: Box::new(target_expr.clone()),
            when_false: Box::new(IRNode::Parenthesized(Box::new(IRNode::assign(
                target_expr,
                value_expr,
            )))),
        })
    }

    /// Build a nullish check conditional
    /// `expr === null || expr === void 0 ? void 0 : value`
    fn build_nullish_check(&self, expr: IRNode, value: IRNode) -> IRNode {
        IRNode::ConditionalExpr {
            condition: Box::new(IRNode::LogicalOr {
                left: Box::new(IRNode::binary(
                    expr.clone(),
                    "===",
                    IRNode::NullLiteral,
                )),
                right: Box::new(IRNode::binary(expr, "===", IRNode::Undefined)),
            }),
            when_true: Box::new(IRNode::Undefined),
            when_false: Box::new(value),
        }
    }

    /// Check if an expression needs a temp variable to avoid double evaluation
    fn is_complex_expression(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        match node.kind {
            // Simple expressions don't need temp
            k if k == SyntaxKind::Identifier as u16 => false,
            k if k == SyntaxKind::ThisKeyword as u16 => false,
            k if k == SyntaxKind::NumericLiteral as u16 => false,
            k if k == SyntaxKind::StringLiteral as u16 => false,
            k if k == SyntaxKind::TrueKeyword as u16 => false,
            k if k == SyntaxKind::FalseKeyword as u16 => false,
            k if k == SyntaxKind::NullKeyword as u16 => false,
            // Complex expressions need temp
            _ => true,
        }
    }

    fn get_identifier_text(&self, idx: NodeIndex) -> String {
        let Some(node) = self.arena.get(idx) else {
            return String::new();
        };
        if let Some(ident) = self.arena.get_identifier(node) {
            return ident.escaped_text.clone();
        }
        String::new()
    }

    fn transform_expression(&self, idx: NodeIndex) -> Option<IRNode> {
        if idx.is_none() {
            return None;
        }

        let node = self.arena.get(idx)?;

        match node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let lit = self.arena.get_literal(node)?;
                Some(IRNode::number(&lit.text))
            }
            k if k == SyntaxKind::StringLiteral as u16 => {
                let lit = self.arena.get_literal(node)?;
                Some(IRNode::string(&lit.text))
            }
            k if k == SyntaxKind::Identifier as u16 => {
                let ident = self.arena.get_identifier(node)?;
                Some(IRNode::id(&ident.escaped_text))
            }
            k if k == SyntaxKind::TrueKeyword as u16 => Some(IRNode::BooleanLiteral(true)),
            k if k == SyntaxKind::FalseKeyword as u16 => Some(IRNode::BooleanLiteral(false)),
            k if k == SyntaxKind::NullKeyword as u16 => Some(IRNode::NullLiteral),
            k if k == SyntaxKind::ThisKeyword as u16 => Some(IRNode::this()),
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.arena.get_access_expr(node)?;
                let object = self.transform_expression(access.expression)?;
                let property = self.get_identifier_text(access.name_or_argument);
                Some(IRNode::prop(object, &property))
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                let access = self.arena.get_access_expr(node)?;
                let object = self.transform_expression(access.expression)?;
                let index = self.transform_expression(access.name_or_argument)?;
                Some(IRNode::elem(object, index))
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                let call = self.arena.get_call_expr(node)?;
                let callee = self.transform_expression(call.expression)?;
                let mut args = Vec::new();
                if let Some(ref arg_list) = call.arguments {
                    for &arg_idx in &arg_list.nodes {
                        if let Some(arg) = self.transform_expression(arg_idx) {
                            args.push(arg);
                        }
                    }
                }
                Some(IRNode::call(callee, args))
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let paren = self.arena.get_parenthesized(node)?;
                let inner = self.transform_expression(paren.expression)?;
                Some(inner.paren())
            }
            _ => Some(IRNode::ASTRef(idx)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::node::NodeArena;

    #[test]
    fn test_transformer_creation() {
        let arena = NodeArena::new();
        let transformer = ES5OptionalChainTransformer::new(&arena);
        assert!(!transformer.is_optional_chain(NodeIndex::NONE));
        assert!(!transformer.is_nullish_coalescing(NodeIndex::NONE));
    }

    #[test]
    fn test_temp_var_generation() {
        let arena = NodeArena::new();
        let mut transformer = ES5OptionalChainTransformer::new(&arena);

        assert_eq!(transformer.next_temp_var(), "_a");
        assert_eq!(transformer.next_temp_var(), "_b");
        assert_eq!(transformer.next_temp_var(), "_c");

        transformer.reset();
        assert_eq!(transformer.next_temp_var(), "_a");
    }

    #[test]
    fn test_nullish_check_ir_structure() {
        let arena = NodeArena::new();
        let transformer = ES5OptionalChainTransformer::new(&arena);

        let result = transformer.build_nullish_check(
            IRNode::id("obj"),
            IRNode::prop(IRNode::id("obj"), "prop"),
        );

        // Should be a conditional expression
        match result {
            IRNode::ConditionalExpr { condition, when_true, when_false } => {
                // when_true should be undefined
                assert!(matches!(*when_true, IRNode::Undefined));
                // when_false should be the property access
                assert!(matches!(*when_false, IRNode::PropertyAccess { .. }));
            }
            _ => panic!("Expected ConditionalExpr"),
        }
    }
}
