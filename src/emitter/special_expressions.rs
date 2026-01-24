//! Special Expression Emission Module
//!
//! This module handles emission of special expressions like yield, await, spread,
//! and decorators that don't fit neatly into other categories.

use super::Printer;
use crate::parser::node::Node;
use crate::scanner::SyntaxKind;

impl<'a> Printer<'a> {
    // =========================================================================
    // Yield and Await
    // =========================================================================

    /// Emit a yield expression: yield or yield* or yield value
    pub(super) fn emit_yield_expression(&mut self, node: &Node) {
        // YieldExpression is stored with UnaryExprData (operand = expression, operator = asterisk flag)
        let Some(unary) = self.arena.get_unary_expr(node) else {
            self.write("yield");
            return;
        };

        self.write("yield");
        // Check if this is yield* (operator stores asterisk flag as SyntaxKind)
        if unary.operator == SyntaxKind::AsteriskToken as u16 {
            self.write("*");
        }
        if !unary.operand.is_none() {
            self.write(" ");
            self.emit_expression(unary.operand);
        }
    }

    /// Emit an await expression: await value
    pub(super) fn emit_await_expression(&mut self, node: &Node) {
        // AwaitExpression is stored with UnaryExprData
        let Some(unary) = self.arena.get_unary_expr(node) else {
            self.write("await");
            return;
        };

        self.write("await ");
        self.emit_expression(unary.operand);
    }

    // =========================================================================
    // Spread Elements
    // =========================================================================

    /// Emit a spread element: ...expr
    pub(super) fn emit_spread_element(&mut self, node: &Node) {
        let Some(spread) = self.arena.get_spread(node) else {
            self.write("...");
            return;
        };

        self.write("...");
        self.emit_expression(spread.expression);
    }

    // =========================================================================
    // Decorators
    // =========================================================================

    /// Emit a decorator: @expression
    pub(super) fn emit_decorator(&mut self, node: &Node) {
        // In ES5 mode, decorators are not supported - skip them entirely
        if self.ctx.target_es5 {
            return;
        }

        let Some(decorator) = self.arena.get_decorator(node) else {
            return;
        };

        self.write("@");
        self.emit(decorator.expression);
    }
}
