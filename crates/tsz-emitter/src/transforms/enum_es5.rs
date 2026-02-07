//! ES5 Enum Transform
//!
//! Transforms TypeScript enums to ES5 IIFE patterns.
//!
//! # Patterns
//!
//! ## Numeric Enum (with Reverse Mapping)
//! ```typescript
//! enum E { A, B = 2 }
//! ```
//! Becomes:
//! ```javascript
//! var E;
//! (function (E) {
//!     E[E["A"] = 0] = "A";
//!     E[E["B"] = 2] = "B";
//! })(E || (E = {}));
//! ```
//!
//! ## String Enum (No Reverse Mapping)
//! ```typescript
//! enum S { A = "a" }
//! ```
//! Becomes:
//! ```javascript
//! var S;
//! (function (S) {
//!     S["A"] = "a";
//! })(S || (S = {}));
//! ```
//!
//! ## Const Enum (Erased by Default)
//! ```typescript
//! const enum CE { A = 0 }
//! // usages are inlined
//! ```

use crate::transforms::ir::*;
use crate::transforms::ir_printer::IRPrinter;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

/// Enum ES5 transformer - produces IR for enum declarations
pub struct EnumES5Transformer<'a> {
    arena: &'a NodeArena,
    /// Track last numeric value for auto-incrementing
    last_value: Option<i64>,
}

impl<'a> EnumES5Transformer<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        EnumES5Transformer {
            arena,
            last_value: None,
        }
    }

    /// Transform an enum declaration to IR
    /// Returns None for const enums (they are erased)
    pub fn transform_enum(&mut self, enum_idx: NodeIndex) -> Option<IRNode> {
        self.last_value = Some(-1); // Start at -1 so first increment is 0

        let Some(enum_node) = self.arena.get(enum_idx) else {
            return None;
        };

        let Some(enum_data) = self.arena.get_enum(enum_node) else {
            return None;
        };

        // Check for const enum - erase by default (preserveConstEnums not yet supported)
        if self.is_const_enum(&enum_data.modifiers) {
            return None;
        }

        let name = self.get_identifier_text(enum_data.name);
        if name.is_empty() {
            return None;
        }

        // Build IR for: var E; (function (E) { ... })(E || (E = {}));
        let mut statements = Vec::new();

        // var E;
        statements.push(IRNode::VarDecl {
            name: name.clone(),
            initializer: None,
        });

        // Build IIFE body (enum member assignments)
        let body = self.transform_members(&enum_data.members, &name);

        // Build IIFE argument: E || (E = {})
        let iife_arg = IRNode::LogicalOr {
            left: Box::new(IRNode::Identifier(name.clone())),
            right: Box::new(IRNode::BinaryExpr {
                left: Box::new(IRNode::Identifier(name.clone())),
                operator: "=".to_string(),
                right: Box::new(IRNode::ObjectLiteral(Vec::new())),
            }),
        };

        // (function (E) { body })(arg)
        let iife = IRNode::CallExpr {
            callee: Box::new(IRNode::FunctionExpr {
                name: None, // IIFEs are anonymous functions
                parameters: vec![IRParam::new(&name)],
                body,
                is_expression_body: false,
                body_source_range: None,
            }),
            arguments: vec![iife_arg],
        };

        statements.push(IRNode::ExpressionStatement(Box::new(iife)));

        Some(IRNode::Sequence(statements))
    }

    /// Get the enum name without transforming
    pub fn get_enum_name(&self, enum_idx: NodeIndex) -> String {
        let Some(enum_node) = self.arena.get(enum_idx) else {
            return String::new();
        };
        let Some(enum_data) = self.arena.get_enum(enum_node) else {
            return String::new();
        };
        self.get_identifier_text(enum_data.name)
    }

    /// Check if enum is a const enum
    pub fn is_const_enum_by_idx(&self, enum_idx: NodeIndex) -> bool {
        let Some(enum_node) = self.arena.get(enum_idx) else {
            return false;
        };
        let Some(enum_data) = self.arena.get_enum(enum_node) else {
            return false;
        };
        self.is_const_enum(&enum_data.modifiers)
    }

    /// Transform enum members to IR statements
    fn transform_members(&mut self, members: &NodeList, enum_name: &str) -> Vec<IRNode> {
        let mut statements = Vec::new();

        for &member_idx in &members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let Some(member_data) = self.arena.get_enum_member(member_node) else {
                continue;
            };

            let member_name = self.get_member_name(member_data.name);
            let has_initializer = !member_data.initializer.is_none();

            let stmt = if has_initializer {
                if self.is_string_literal(member_data.initializer) {
                    // String enum: E["A"] = "val";
                    // No reverse mapping for string enums
                    let assign = IRNode::BinaryExpr {
                        left: Box::new(IRNode::ElementAccess {
                            object: Box::new(IRNode::Identifier(enum_name.to_string())),
                            index: Box::new(IRNode::StringLiteral(member_name.clone())),
                        }),
                        operator: "=".to_string(),
                        right: Box::new(self.transform_expression(member_data.initializer)),
                    };
                    self.last_value = None; // Reset auto-increment
                    IRNode::ExpressionStatement(Box::new(assign))
                } else {
                    // Numeric/Computed: E[E["A"] = val] = "A";
                    // Try to evaluate the constant expression for auto-increment tracking
                    if let Some(evaluated) =
                        self.evaluate_constant_expression(member_data.initializer)
                    {
                        self.last_value = Some(evaluated);
                    } else {
                        self.last_value = None; // Can't evaluate, reset auto-increment
                    }
                    let inner_value = self.transform_expression(member_data.initializer);
                    let inner_assign = IRNode::BinaryExpr {
                        left: Box::new(IRNode::ElementAccess {
                            object: Box::new(IRNode::Identifier(enum_name.to_string())),
                            index: Box::new(IRNode::StringLiteral(member_name.clone())),
                        }),
                        operator: "=".to_string(),
                        right: Box::new(inner_value),
                    };
                    let outer_assign = IRNode::BinaryExpr {
                        left: Box::new(IRNode::ElementAccess {
                            object: Box::new(IRNode::Identifier(enum_name.to_string())),
                            index: Box::new(inner_assign),
                        }),
                        operator: "=".to_string(),
                        right: Box::new(IRNode::StringLiteral(member_name.clone())),
                    };
                    IRNode::ExpressionStatement(Box::new(outer_assign))
                }
            } else {
                // Auto-increment: E[E["A"] = 0] = "A";
                let next_val = self.last_value.map(|v| v + 1).unwrap_or(0);
                self.last_value = Some(next_val);

                let inner_assign = IRNode::BinaryExpr {
                    left: Box::new(IRNode::ElementAccess {
                        object: Box::new(IRNode::Identifier(enum_name.to_string())),
                        index: Box::new(IRNode::StringLiteral(member_name.clone())),
                    }),
                    operator: "=".to_string(),
                    right: Box::new(IRNode::NumericLiteral(next_val.to_string())),
                };
                let outer_assign = IRNode::BinaryExpr {
                    left: Box::new(IRNode::ElementAccess {
                        object: Box::new(IRNode::Identifier(enum_name.to_string())),
                        index: Box::new(inner_assign),
                    }),
                    operator: "=".to_string(),
                    right: Box::new(IRNode::StringLiteral(member_name.clone())),
                };
                IRNode::ExpressionStatement(Box::new(outer_assign))
            };

            statements.push(stmt);
        }

        statements
    }

    /// Transform an expression node to IR
    fn transform_expression(&self, idx: NodeIndex) -> IRNode {
        let Some(node) = self.arena.get(idx) else {
            return IRNode::NumericLiteral("0".to_string());
        };

        match node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    IRNode::NumericLiteral(lit.text.clone())
                } else {
                    IRNode::NumericLiteral("0".to_string())
                }
            }
            k if k == SyntaxKind::StringLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    IRNode::StringLiteral(lit.text.clone())
                } else {
                    IRNode::StringLiteral(String::new())
                }
            }
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(id) = self.arena.get_identifier(node) {
                    IRNode::Identifier(id.escaped_text.clone())
                } else {
                    IRNode::Identifier("unknown".to_string())
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.arena.get_binary_expr(node) {
                    IRNode::BinaryExpr {
                        left: Box::new(self.transform_expression(bin.left)),
                        operator: self.emit_operator(bin.operator_token),
                        right: Box::new(self.transform_expression(bin.right)),
                    }
                } else {
                    IRNode::NumericLiteral("0".to_string())
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.arena.get_unary_expr(node) {
                    IRNode::PrefixUnaryExpr {
                        operator: self.emit_operator(unary.operator),
                        operand: Box::new(self.transform_expression(unary.operand)),
                    }
                } else {
                    IRNode::NumericLiteral("0".to_string())
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.arena.get_parenthesized(node) {
                    IRNode::Parenthesized(Box::new(self.transform_expression(paren.expression)))
                } else {
                    IRNode::NumericLiteral("0".to_string())
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                // E.A reference inside enum
                if let Some(access) = self.arena.get_access_expr(node) {
                    let obj = self.transform_expression(access.expression);
                    let prop = if let Some(prop_node) = self.arena.get(access.name_or_argument) {
                        if let Some(ident) = self.arena.get_identifier(prop_node) {
                            ident.escaped_text.clone()
                        } else if let Some(lit) = self.arena.get_literal(prop_node) {
                            lit.text.clone()
                        } else {
                            "unknown".to_string()
                        }
                    } else {
                        "unknown".to_string()
                    };
                    IRNode::PropertyAccess {
                        object: Box::new(obj),
                        property: prop,
                    }
                } else {
                    IRNode::NumericLiteral("0".to_string())
                }
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node) {
                    IRNode::ElementAccess {
                        object: Box::new(self.transform_expression(access.expression)),
                        index: Box::new(self.transform_expression(access.name_or_argument)),
                    }
                } else {
                    IRNode::NumericLiteral("0".to_string())
                }
            }
            _ => {
                // Fallback - return 0
                IRNode::NumericLiteral("0".to_string())
            }
        }
    }

    fn emit_operator(&self, op: u16) -> String {
        match op {
            k if k == SyntaxKind::PlusToken as u16 => "+",
            k if k == SyntaxKind::MinusToken as u16 => "-",
            k if k == SyntaxKind::AsteriskToken as u16 => "*",
            k if k == SyntaxKind::SlashToken as u16 => "/",
            k if k == SyntaxKind::PercentToken as u16 => "%",
            k if k == SyntaxKind::LessThanLessThanToken as u16 => "<<",
            k if k == SyntaxKind::GreaterThanGreaterThanToken as u16 => ">>",
            k if k == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16 => ">>>",
            k if k == SyntaxKind::AmpersandToken as u16 => "&",
            k if k == SyntaxKind::BarToken as u16 => "|",
            k if k == SyntaxKind::CaretToken as u16 => "^",
            k if k == SyntaxKind::TildeToken as u16 => "~",
            k if k == SyntaxKind::ExclamationToken as u16 => "!",
            _ => "+",
        }
        .to_string()
    }

    fn is_const_enum(&self, modifiers: &Option<NodeList>) -> bool {
        if let Some(mods) = modifiers {
            for &idx in &mods.nodes {
                if let Some(node) = self.arena.get(idx)
                    && node.kind == SyntaxKind::ConstKeyword as u16
                {
                    return true;
                }
            }
        }
        false
    }

    /// Try to evaluate a constant expression to its numeric value
    /// Returns None if the expression can't be statically evaluated
    fn evaluate_constant_expression(&self, idx: NodeIndex) -> Option<i64> {
        let node = self.arena.get(idx)?;

        match node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let lit = self.arena.get_literal(node)?;
                lit.text.parse().ok()
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                let bin = self.arena.get_binary_expr(node)?;
                let left = self.evaluate_constant_expression(bin.left)?;
                let right = self.evaluate_constant_expression(bin.right)?;
                let op = bin.operator_token;

                match op {
                    o if o == SyntaxKind::PlusToken as u16 => left.checked_add(right),
                    o if o == SyntaxKind::MinusToken as u16 => left.checked_sub(right),
                    o if o == SyntaxKind::AsteriskToken as u16 => left.checked_mul(right),
                    o if o == SyntaxKind::SlashToken as u16 => {
                        if right != 0 {
                            Some(left / right)
                        } else {
                            None
                        }
                    }
                    o if o == SyntaxKind::PercentToken as u16 => {
                        if right != 0 {
                            Some(left % right)
                        } else {
                            None
                        }
                    }
                    o if o == SyntaxKind::LessThanLessThanToken as u16 => {
                        Some(left.wrapping_shl(right as u32))
                    }
                    o if o == SyntaxKind::GreaterThanGreaterThanToken as u16 => {
                        Some(left.wrapping_shr(right as u32))
                    }
                    o if o == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16 => {
                        Some((left as u64).wrapping_shr(right as u32) as i64)
                    }
                    o if o == SyntaxKind::AmpersandToken as u16 => Some(left & right),
                    o if o == SyntaxKind::BarToken as u16 => Some(left | right),
                    o if o == SyntaxKind::CaretToken as u16 => Some(left ^ right),
                    _ => None,
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.arena.get_unary_expr(node)?;
                let operand = self.evaluate_constant_expression(unary.operand)?;
                let op = unary.operator;
                match op {
                    o if o == SyntaxKind::MinusToken as u16 => Some(operand.checked_neg()?),
                    o if o == SyntaxKind::TildeToken as u16 => Some(!operand),
                    o if o == SyntaxKind::ExclamationToken as u16 => {
                        Some(if operand == 0 { 1 } else { 0 })
                    }
                    o if o == SyntaxKind::PlusToken as u16 => Some(operand),
                    _ => None,
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let paren = self.arena.get_parenthesized(node)?;
                self.evaluate_constant_expression(paren.expression)
            }
            _ => None,
        }
    }

    fn is_string_literal(&self, idx: NodeIndex) -> bool {
        if let Some(node) = self.arena.get(idx) {
            return node.kind == SyntaxKind::StringLiteral as u16;
        }
        false
    }

    fn get_identifier_text(&self, idx: NodeIndex) -> String {
        if let Some(node) = self.arena.get(idx)
            && let Some(ident) = self.arena.get_identifier(node)
        {
            return ident.escaped_text.clone();
        }
        String::new()
    }

    fn get_member_name(&self, idx: NodeIndex) -> String {
        if let Some(node) = self.arena.get(idx) {
            // Can be identifier or string literal for computed names
            if let Some(ident) = self.arena.get_identifier(node) {
                return ident.escaped_text.clone();
            }
            if let Some(lit) = self.arena.get_literal(node) {
                return lit.text.clone();
            }
        }
        String::new()
    }
}

/// Legacy enum ES5 emitter for backward compatibility
/// Deprecated: Use EnumES5Transformer + IRPrinter instead
#[allow(dead_code)] // Legacy infrastructure, kept for compatibility
pub struct EnumES5Emitter<'a> {
    arena: &'a NodeArena,
    output: String,
    indent_level: u32,
    transformer: EnumES5Transformer<'a>,
}

impl<'a> EnumES5Emitter<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        EnumES5Emitter {
            arena,
            output: String::with_capacity(1024),
            indent_level: 0,
            transformer: EnumES5Transformer::new(arena),
        }
    }

    pub fn set_indent_level(&mut self, level: u32) {
        self.indent_level = level;
    }

    /// Emit an enum declaration
    /// Returns empty string for const enums (they are erased)
    pub fn emit_enum(&mut self, enum_idx: NodeIndex) -> String {
        self.output.clear();

        let ir = self.transformer.transform_enum(enum_idx);
        let ir = match ir {
            Some(ir) => ir,
            None => return String::new(),
        };

        let mut printer = IRPrinter::new();
        printer.set_indent_level(self.indent_level);
        let result = printer.emit(&ir);
        result.to_string()
    }

    /// Get the enum name without emitting anything
    pub fn get_enum_name(&self, enum_idx: NodeIndex) -> String {
        self.transformer.get_enum_name(enum_idx)
    }

    /// Check if enum is a const enum
    pub fn is_const_enum_by_idx(&self, enum_idx: NodeIndex) -> bool {
        self.transformer.is_const_enum_by_idx(enum_idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_parser::parser::ParserState;

    fn transform_enum(source: &str) -> String {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        if let Some(root_node) = parser.arena.get(root)
            && let Some(source_file) = parser.arena.get_source_file(root_node)
            && let Some(&enum_idx) = source_file.statements.nodes.first()
        {
            let mut transformer = EnumES5Transformer::new(&parser.arena);
            if let Some(ir) = transformer.transform_enum(enum_idx) {
                return IRPrinter::emit_to_string(&ir);
            }
        }
        String::new()
    }

    fn emit_enum_legacy(source: &str) -> String {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        if let Some(root_node) = parser.arena.get(root)
            && let Some(source_file) = parser.arena.get_source_file(root_node)
            && let Some(&enum_idx) = source_file.statements.nodes.first()
        {
            let mut emitter = EnumES5Emitter::new(&parser.arena);
            return emitter.emit_enum(enum_idx);
        }
        String::new()
    }

    #[test]
    fn test_numeric_enum() {
        let output = transform_enum("enum E { A, B, C }");
        assert!(output.contains("var E;"), "Should declare var E");
        assert!(output.contains("(function (E)"), "Should have IIFE");
        assert!(
            output.contains("E[E[\"A\"] = 0] = \"A\""),
            "Should have reverse mapping for A"
        );
        assert!(
            output.contains("E[E[\"B\"] = 1] = \"B\""),
            "Should have reverse mapping for B"
        );
        assert!(
            output.contains("E[E[\"C\"] = 2] = \"C\""),
            "Should auto-increment C"
        );
    }

    #[test]
    fn test_enum_with_initializer() {
        let output = transform_enum("enum E { A = 10, B, C = 20 }");
        assert!(
            output.contains("E[E[\"A\"] = 10] = \"A\""),
            "A should be 10"
        );
        assert!(
            output.contains("E[E[\"B\"] = 11] = \"B\""),
            "B should be 11 (auto-increment)"
        );
        assert!(
            output.contains("E[E[\"C\"] = 20] = \"C\""),
            "C should be 20"
        );
    }

    #[test]
    fn test_string_enum() {
        let output = transform_enum("enum S { A = \"alpha\", B = \"beta\" }");
        assert!(output.contains("var S;"), "Should declare var S");
        assert!(
            output.contains("S[\"A\"] = \"alpha\";"),
            "String enum no reverse mapping"
        );
        assert!(
            output.contains("S[\"B\"] = \"beta\";"),
            "String enum no reverse mapping"
        );
        // Should NOT contain reverse mapping pattern
        assert!(
            !output.contains("S[S["),
            "String enums should not have reverse mapping"
        );
    }

    #[test]
    fn test_const_enum_erased() {
        let output = transform_enum("const enum CE { A = 0 }");
        assert!(
            output.trim().is_empty(),
            "Const enums should be erased: {}",
            output
        );
    }

    #[test]
    fn test_legacy_emitter_produces_same_output() {
        // Test that the legacy wrapper produces the same output
        let new_output = transform_enum("enum E { A, B = 2 }");
        let legacy_output = emit_enum_legacy("enum E { A, B = 2 }");
        assert_eq!(
            new_output, legacy_output,
            "Legacy and new output should match"
        );
    }

    #[test]
    fn test_enum_with_binary_expression() {
        let output = transform_enum("enum E { A = 1 + 2, B }");
        assert!(output.contains("var E;"), "Should declare var E");
        assert!(
            output.contains("E[E[\"A\"] = 1 + 2] = \"A\""),
            "Should handle binary expression"
        );
        assert!(
            output.contains("E[E[\"B\"] = 4] = \"B\""),
            "Should auto-increment after computed value (A=3, so B=4)"
        );
    }

    #[test]
    fn test_enum_with_unary_expression() {
        let output = transform_enum("enum E { A = -5 }");
        assert!(output.contains("var E;"), "Should declare var E");
        assert!(
            output.contains("E[E[\"A\"] = -5] = \"A\""),
            "Should handle unary expression"
        );
    }

    #[test]
    fn test_enum_with_property_access() {
        let output = transform_enum("enum E { A = E.B }");
        assert!(output.contains("var E;"), "Should declare var E");
        // Property access should be preserved
        assert!(output.contains("E.B"), "Should preserve property access");
    }
}
