//! Special Expression Emission Module
//!
//! This module handles emission of special expressions like yield, await, spread,
//! and decorators that don't fit neatly into other categories.

#![allow(clippy::print_stderr)]

use super::Printer;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    // =========================================================================
    // Yield and Await
    // =========================================================================

    /// Emit a yield expression: yield or yield* or yield value
    pub(super) fn emit_yield_expression(&mut self, node: &Node) {
        // YieldExpression is stored with UnaryExprDataEx (expression + asterisk_token)
        let Some(unary) = self.arena.get_unary_expr_ex(node) else {
            self.write("yield");
            return;
        };

        self.write("yield");
        if unary.asterisk_token {
            self.write("*");
        }
        if !unary.expression.is_none() {
            self.write(" ");
            self.emit_expression(unary.expression);
        } else if unary.asterisk_token {
            // TypeScript emits `yield* ;` (with space) when yield* has no expression
            self.write(" ");
        }
    }

    /// Emit an await expression: await value
    pub(super) fn emit_await_expression(&mut self, node: &Node) {
        // AwaitExpression is stored with UnaryExprDataEx
        let Some(unary) = self.arena.get_unary_expr_ex(node) else {
            self.write("await");
            return;
        };

        self.write("await ");
        self.emit_expression(unary.expression);
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
    ///
    /// For ES6+ targets, decorators are emitted as native `@expression` syntax.
    /// For ES5 targets, decorators should be downleveled using the `__decorate` helper,
    /// but this requires class-level coordination. Standalone decorator emission
    /// in ES5 mode is handled here by emitting a warning comment.
    ///
    /// Note: Full decorator ES5 lowering is handled at the class level by collecting
    /// decorators and emitting `__decorate([...], Class, "member", descriptor)` calls
    /// after the class definition.
    pub(super) fn emit_decorator(&mut self, node: &Node) {
        let Some(decorator) = self.arena.get_decorator(node) else {
            return;
        };

        // In ES5 mode, standalone decorator nodes should not be emitted directly.
        // Decorator lowering is handled at the class level via __decorate helper.
        // If we encounter a decorator here in ES5 mode, emit a warning comment.
        if self.ctx.target_es5 {
            // Get decorator expression text for the warning
            let decorator_text = self.get_decorator_text(decorator.expression);
            eprintln!(
                "Warning: Decorator @{} skipped in ES5 mode - decorator lowering not fully implemented",
                decorator_text
            );
            self.write("/* @");
            self.write(&decorator_text);
            self.write(" - ES5 decorator lowering not implemented */");
            return;
        }

        // ES6+ native decorator syntax
        self.write("@");
        self.emit(decorator.expression);
    }

    /// Maximum recursion depth for decorator text extraction to prevent stack overflow
    const MAX_DECORATOR_TEXT_DEPTH: u32 = 10;

    /// Get a string representation of a decorator expression for diagnostics
    fn get_decorator_text(&self, expr_idx: tsz_parser::parser::NodeIndex) -> String {
        self.get_decorator_text_with_depth(expr_idx, 0)
    }

    /// Get a string representation of a decorator expression with depth limiting
    fn get_decorator_text_with_depth(
        &self,
        expr_idx: tsz_parser::parser::NodeIndex,
        depth: u32,
    ) -> String {
        // Prevent unbounded recursion for deeply nested decorator expressions
        if depth > Self::MAX_DECORATOR_TEXT_DEPTH {
            return "...".to_string();
        }

        if expr_idx.is_none() {
            return "unknown".to_string();
        }

        let Some(expr_node) = self.arena.get(expr_idx) else {
            return "unknown".to_string();
        };

        // Handle common cases: identifier, call expression
        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(ident) = self.arena.get_identifier(expr_node) {
                    return ident.escaped_text.clone();
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(expr_node) {
                    let callee_text =
                        self.get_decorator_text_with_depth(call.expression, depth + 1);
                    return format!("{}(...)", callee_text);
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(expr_node) {
                    let obj_text = self.get_decorator_text_with_depth(access.expression, depth + 1);
                    let prop_text =
                        self.get_decorator_text_with_depth(access.name_or_argument, depth + 1);
                    return format!("{}.{}", obj_text, prop_text);
                }
            }
            _ => {}
        }

        "expression".to_string()
    }
}
