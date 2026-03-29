//! IR Printer - Emits JavaScript strings from IR nodes
//!
//! This module handles all string emission from the IR. Transforms produce IR nodes,
//! and this printer converts them to JavaScript strings.
//!
//! # Example
//!
//! ```ignore
//! use crate::transforms::ir::IRNode;
//! use crate::transforms::ir_printer::IRPrinter;
//!
//! let ir = IRNode::func_decl("foo", vec![], vec![
//!     IRNode::ret(Some(IRNode::number("42")))
//! ]);
//!
//! let mut printer = IRPrinter::new();
//! let output = printer.emit(&ir);
//! // output: "function foo() {\n    return 42;\n}"
//! ```

use std::borrow::Cow;
use std::fmt::Write;

#[path = "ir_printer_helpers.rs"]
mod ir_printer_helpers;

use crate::context::transform::TransformContext;
use crate::emitter::{Printer as AstPrinter, PrinterOptions};
use crate::transforms::ir::{
    EnumMember, EnumMemberValue, IRMethodName, IRNode, IRParam, IRProperty, IRPropertyKey,
    IRPropertyKind, IRSwitchCase,
};
use tsz_parser::parser::base::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::syntax_kind_ext;

#[derive(Clone, Copy)]
struct NamespaceIifeContext<'a> {
    is_exported: bool,
    attach_to_exports: bool,
    should_declare_var: bool,
    parent_name: Option<&'a str>,
    param_name: Option<&'a str>,
}

/// IR Printer - converts IR nodes to JavaScript strings
pub struct IRPrinter<'a> {
    output: String,
    indent_level: u32,
    indent_str: &'static str,
    /// Optional arena for handling `ASTRef` nodes
    arena: Option<&'a NodeArena>,
    /// Source text for emitting `ASTRef` nodes
    source_text: Option<&'a str>,
    /// Optional transform directives for `ASTRef` nodes
    transforms: Option<TransformContext>,
    /// Avoid duplicate trailing comments when a sequence explicitly carries one.
    suppress_function_trailing_extraction: bool,
    /// Name of the current ES5 class IIFE constructor, used to force constructor
    /// empty-body formatting without affecting nested function declarations.
    current_class_iife_name: Option<String>,
    /// When true, the next `FunctionExpr` emit will force multiline for empty bodies.
    /// Set by `CallExpr` when emitting an IIFE callee.
    force_iife_multiline_empty: bool,
    /// When true, we are inside a namespace IIFE body.
    /// Nested namespace variable declarations use `let` instead of `var` in ES2015+ targets.
    in_namespace_iife_body: bool,
    /// When true, the target is ES5 and `let`/`const` should not be emitted.
    target_es5: bool,
    /// When true, comments like `/** @class */` are suppressed in output.
    remove_comments: bool,
    /// When true, prefix runtime helper calls with `tslib_1.` (for CJS importHelpers).
    tslib_prefix: bool,
}

impl<'a> IRPrinter<'a> {
    fn enum_with_matching_namespace_export<'b>(
        first: &'b IRNode,
        second: &'b IRNode,
    ) -> Option<(&'b str, &'b Vec<EnumMember>, &'b str)> {
        let IRNode::EnumIIFE { name, members, .. } = first else {
            return None;
        };
        let IRNode::NamespaceExport {
            namespace,
            name: export_name,
            value,
        } = second
        else {
            return None;
        };
        let IRNode::Identifier(identifier_name) = &**value else {
            return None;
        };
        (export_name == name && identifier_name == name).then_some((&**name, members, &**namespace))
    }

    /// Check if a node is a `return [opcode ...];` generator op return statement.
    /// Used to decide whether to inline `case N: return [opcode];` on one line.
    fn is_generator_return(node: &IRNode) -> bool {
        matches!(
            node,
            IRNode::ReturnStatement(Some(expr)) if matches!(expr.as_ref(), IRNode::GeneratorOp { .. })
        )
    }

    fn emit_namespace_bound_enum_iife(
        &mut self,
        enum_name: &str,
        members: &[EnumMember],
        namespace: &str,
    ) {
        // Inside namespace body, use `let` (ES2015+ block scoping).
        // ES5 doesn't support `let`, so must always use `var`.
        let keyword = if self.in_namespace_iife_body && !self.target_es5 {
            "let"
        } else {
            "var"
        };
        self.write(keyword);
        self.write(" ");
        self.write(enum_name);
        self.write(";");
        self.write_line();
        self.write_indent();
        self.write("(function (");
        self.write(enum_name);
        self.write(") {");
        self.write_line();
        self.increase_indent();
        for member in members {
            self.write_indent();
            self.emit_enum_member(enum_name, member);
            self.write_line();
        }
        self.decrease_indent();
        self.write_indent();
        self.write("})(");
        self.write(enum_name);
        self.write(" = ");
        self.write(namespace);
        self.write(".");
        self.write(enum_name);
        self.write(" || (");
        self.write(namespace);
        self.write(".");
        self.write(enum_name);
        self.write(" = {}));");
    }

    fn extract_trailing_comment_from_function(&self, function: &IRNode) -> Option<String> {
        let source_text = self.source_text?;
        let (body_start, body_end) = match function {
            IRNode::FunctionExpr {
                body_source_range: Some((body_start, body_end)),
                ..
            }
            | IRNode::FunctionDecl {
                body_source_range: Some((body_start, body_end)),
                ..
            } => (*body_start, *body_end),
            _ => return None,
        };
        let bytes = source_text.as_bytes();
        let start = body_start as usize;
        let end = (body_end as usize).min(bytes.len());
        if start >= end {
            return None;
        }
        let mut trailing = None;
        for (offset, &byte) in bytes[start..end].iter().enumerate() {
            if byte == b'}'
                && let Some(comment) =
                    crate::emitter::get_trailing_comment_ranges(source_text, start + offset + 1)
                        .first()
            {
                trailing =
                    Some(source_text[comment.pos as usize..comment.end as usize].to_string());
            }
        }
        trailing
    }

    fn should_indent_sequence_child(node: &IRNode) -> bool {
        match node {
            IRNode::NamespaceIIFE {
                skip_sequence_indent,
                ..
            } => !skip_sequence_indent,
            _ => true,
        }
    }

    fn is_noop_statement(node: &IRNode) -> bool {
        match node {
            IRNode::Sequence(nodes) if nodes.is_empty() => true,
            IRNode::EmptyStatement => true,
            IRNode::Raw(text) => text.trim().is_empty(),
            _ => false,
        }
    }

    /// Create a new IR printer
    pub fn new() -> Self {
        Self {
            output: String::with_capacity(4096),
            indent_level: 0,
            indent_str: "    ",
            arena: None,
            source_text: None,
            transforms: None,
            suppress_function_trailing_extraction: false,
            current_class_iife_name: None,
            force_iife_multiline_empty: false,
            in_namespace_iife_body: false,
            target_es5: false,
            remove_comments: false,
            tslib_prefix: false,
        }
    }

    /// Create an IR printer with an arena for `ASTRef` handling
    pub fn with_arena(arena: &'a NodeArena) -> Self {
        Self {
            output: String::with_capacity(4096),
            indent_level: 0,
            indent_str: "    ",
            arena: Some(arena),
            source_text: None,
            transforms: None,
            suppress_function_trailing_extraction: false,
            current_class_iife_name: None,
            force_iife_multiline_empty: false,
            in_namespace_iife_body: false,
            target_es5: false,
            remove_comments: false,
            tslib_prefix: false,
        }
    }

    /// Create an IR printer with both arena and source text for `ASTRef` emission
    pub fn with_arena_and_source(arena: &'a NodeArena, source_text: &'a str) -> Self {
        Self {
            output: String::with_capacity(4096),
            indent_level: 0,
            indent_str: "    ",
            arena: Some(arena),
            source_text: Some(source_text),
            transforms: None,
            suppress_function_trailing_extraction: false,
            current_class_iife_name: None,
            force_iife_multiline_empty: false,
            in_namespace_iife_body: false,
            target_es5: false,
            remove_comments: false,
            tslib_prefix: false,
        }
    }

    /// Set transform directives for `ASTRef` emission
    pub fn set_transforms(&mut self, transforms: TransformContext) {
        self.transforms = Some(transforms);
    }

    /// Enable `tslib_1.` prefix for runtime helper calls (importHelpers + CJS).
    pub const fn set_tslib_prefix(&mut self, enable: bool) {
        self.tslib_prefix = enable;
    }

    /// Write a runtime helper name, prefixing with `tslib_1.` when `tslib_prefix` is active.
    fn write_helper(&mut self, name: &str) {
        if self.tslib_prefix {
            self.output.push_str("tslib_1.");
        }
        self.output.push_str(name);
    }

    /// Set the source text for `ASTRef` emission
    pub const fn set_source_text(&mut self, text: &'a str) {
        self.source_text = Some(text);
    }

    /// Set the indentation level
    pub const fn set_indent_level(&mut self, level: u32) {
        self.indent_level = level;
    }

    /// Mark this printer as targeting ES5 (disables `let`/`const` emission).
    pub const fn set_target_es5(&mut self, es5: bool) {
        self.target_es5 = es5;
    }

    /// When true, suppress comment annotations like `/** @class */` in output.
    pub const fn set_remove_comments(&mut self, remove: bool) {
        self.remove_comments = remove;
    }

    /// Get the output
    pub fn get_output(&self) -> &str {
        &self.output
    }

    /// Take the output
    pub fn take_output(self) -> String {
        self.output
    }

    /// Emit an IR node to a string
    pub fn emit(&mut self, node: &IRNode) -> &str {
        // For top-level Sequences, add newlines between statements
        if let IRNode::Sequence(nodes) = node {
            let mut i = 0;
            while i < nodes.len() {
                if i > 0 {
                    self.write_line();
                    if Self::should_indent_sequence_child(&nodes[i]) {
                        self.write_indent();
                    }
                }
                if i + 1 < nodes.len()
                    && let Some((enum_name, members, namespace)) =
                        Self::enum_with_matching_namespace_export(&nodes[i], &nodes[i + 1])
                {
                    self.emit_namespace_bound_enum_iife(enum_name, members, namespace);
                    i += 2;
                    continue;
                }
                let suppress_for_this_node = i + 1 < nodes.len()
                    && matches!(&nodes[i], IRNode::FunctionDecl { .. })
                    && matches!(&nodes[i + 1], IRNode::TrailingComment(_));
                let prev_suppress = self.suppress_function_trailing_extraction;
                self.suppress_function_trailing_extraction = suppress_for_this_node;
                self.emit_node(&nodes[i]);
                self.suppress_function_trailing_extraction = prev_suppress;
                i += 1;
            }
        } else {
            self.emit_node(node);
        }
        &self.output
    }

    /// Emit an IR node and return the output
    pub fn emit_to_string(node: &IRNode) -> String {
        let mut printer = Self::new();
        printer.emit(node);
        printer.output
    }

    /// Check whether a property access on `node` needs `..` instead of `.`.
    /// Plain decimal integer literals need `..` because `0.x` would be
    /// parsed as the float `0.` followed by identifier `x`.
    fn ir_node_needs_double_dot(node: &IRNode) -> bool {
        match node {
            IRNode::NumericLiteral(n) => {
                let num_text = n.trim();
                let is_prefixed = num_text.starts_with("0x")
                    || num_text.starts_with("0X")
                    || num_text.starts_with("0o")
                    || num_text.starts_with("0O")
                    || num_text.starts_with("0b")
                    || num_text.starts_with("0B");
                !is_prefixed
                    && !num_text.contains('.')
                    && !num_text.contains('e')
                    && !num_text.contains('E')
            }
            // Other expressions (including parenthesized) never need double-dot
            // because the closing paren already disambiguates: `(1).foo` is valid JS.
            _ => false,
        }
    }

    fn emit_node(&mut self, node: &IRNode) {
        match node {
            // Literals
            IRNode::NumericLiteral(n) => self.write(n),
            IRNode::StringLiteral(s) => {
                self.write("\"");
                self.write_escaped(s);
                self.write("\"");
            }
            IRNode::RawStringLiteral(s) => {
                self.write("\"");
                self.write(s);
                self.write("\"");
            }
            IRNode::BooleanLiteral(b) => {
                self.write(if *b { "true" } else { "false" });
            }
            IRNode::NullLiteral => self.write("null"),
            IRNode::Undefined => self.write("void 0"),

            // Identifiers
            IRNode::Identifier(name) => self.write(name),
            IRNode::This { captured } => {
                self.write(if *captured { "_this" } else { "this" });
            }
            IRNode::Super => self.write("super"),

            // Expressions
            IRNode::BinaryExpr {
                left,
                operator,
                right,
            } => {
                self.emit_node(left);
                if *operator == "," {
                    self.write(", ");
                } else {
                    self.write(" ");
                    self.write(operator);
                    self.write(" ");
                }
                self.emit_node(right);
            }
            IRNode::PrefixUnaryExpr { operator, operand } => {
                self.write(operator);
                self.emit_node(operand);
            }
            IRNode::PostfixUnaryExpr { operand, operator } => {
                self.emit_node(operand);
                self.write(operator);
            }
            IRNode::CallExpr { callee, arguments } => {
                // Check if this is an IIFE (immediately invoked function expression)
                // IIFEs should be wrapped in parentheses: (function() { ... })(args)
                let is_iife = matches!(&**callee, IRNode::FunctionExpr { .. });
                if is_iife {
                    self.write("(");
                    // IIFE function bodies should use multiline for empty bodies,
                    // matching TSC's behavior for synthetic code wrappers.
                    self.force_iife_multiline_empty = true;
                }
                self.emit_node(callee);
                self.force_iife_multiline_empty = false;
                if is_iife {
                    self.write(")");
                }
                self.write("(");
                self.emit_comma_separated(arguments);
                self.write(")");
            }
            IRNode::NewExpr {
                callee,
                arguments,
                explicit_arguments,
            } => {
                self.write("new ");
                self.emit_node(callee);
                if *explicit_arguments {
                    self.write("(");
                    self.emit_comma_separated(arguments);
                    self.write(")");
                }
            }
            IRNode::PropertyAccess { object, property } => {
                self.emit_node(object);
                if Self::ir_node_needs_double_dot(object) {
                    self.write("..");
                } else {
                    self.write(".");
                }
                self.write(property);
            }
            IRNode::ElementAccess { object, index } => {
                self.emit_node(object);
                self.write("[");
                self.emit_node(index);
                self.write("]");
            }
            IRNode::ConditionalExpr {
                condition,
                when_true,
                when_false,
            } => {
                self.emit_node(condition);
                self.write(" ? ");
                self.emit_node(when_true);
                self.write(" : ");
                self.emit_node(when_false);
            }
            IRNode::Parenthesized(expr) => {
                self.write("(");
                self.emit_node(expr);
                self.write(")");
            }
            IRNode::CommaExpr(exprs) => {
                self.write("(");
                self.emit_comma_separated(exprs);
                self.write(")");
            }
            IRNode::CommaExprMultiline(exprs) => {
                // Multiline comma expression for ES5 computed property lowering:
                // (_a = {},
                //     _a[key] = value,
                //     _a)
                self.write("(");
                self.indent_level += 1;
                for (i, expr) in exprs.iter().enumerate() {
                    if i > 0 {
                        self.write(",");
                        self.write_line();
                        self.write_indent();
                    }
                    self.emit_node(expr);
                }
                self.indent_level -= 1;
                self.write(")");
            }
            IRNode::ArrayLiteral(elements) => {
                self.write("[");
                self.emit_comma_separated(elements);
                self.write("]");
            }
            IRNode::SpreadElement(expr) => {
                self.write("...");
                self.emit_node(expr);
            }
            IRNode::ObjectLiteral {
                properties,
                source_range,
            } => {
                if properties.is_empty() {
                    self.write("{}");
                    return;
                }

                // Check if the object was multiline in source
                let is_multiline = if let Some((pos, end)) = source_range {
                    !self.is_single_line_range(*pos, *end)
                } else {
                    false
                };

                if is_multiline {
                    // Multiline format
                    self.write("{");
                    self.write_line();
                    self.indent_level += 1;
                    for (i, prop) in properties.iter().enumerate() {
                        self.write_indent();
                        self.emit_property(prop);
                        if i < properties.len() - 1 {
                            self.write(",");
                        }
                        self.write_line();
                    }
                    self.indent_level -= 1;
                    self.write_indent();
                    self.write("}");
                } else {
                    // Single-line format
                    self.write("{ ");
                    for (i, prop) in properties.iter().enumerate() {
                        if i > 0 {
                            self.write(", ");
                        }
                        self.emit_property(prop);
                    }
                    self.write(" }");
                }
            }
            IRNode::FunctionExpr {
                name,
                parameters,
                body,
                is_expression_body,
                body_source_range,
            } => {
                self.write("function ");
                if let Some(n) = name {
                    self.write(n);
                }
                self.write("(");
                self.emit_parameters(parameters);
                self.write(") ");
                let has_defaults = parameters.iter().any(|p| p.default_value.is_some());
                let is_source_single_line = self.is_body_source_single_line(*body_source_range);

                // Single-line function body: { return expr; }
                // Applies to:
                // 1. Arrow-to-function conversions (is_expression_body)
                // 2. Functions that were single-line in source (is_source_single_line)
                // tsc never collapses multi-line function bodies to single line,
                // so we should NOT use heuristics to guess single-line for generated code.
                let should_emit_single_line = *is_expression_body || is_source_single_line;

                let has_rest_to_lower = self.target_es5 && parameters.iter().any(|p| p.rest);
                if !has_defaults
                    && !has_rest_to_lower
                    && should_emit_single_line
                    && body.len() == 1
                    && match &body[0] {
                        IRNode::ReturnStatement(Some(expr)) => {
                            self.write("{ return ");
                            self.emit_node(expr);
                            self.write("; }");
                            true
                        }
                        IRNode::ExpressionStatement(expr) => {
                            self.write("{ ");
                            self.emit_node(expr);
                            self.write("; }");
                            true
                        }
                        _ => false,
                    }
                {
                    return;
                }
                let force_multiline_empty = self.force_iife_multiline_empty
                    || matches!(name, Some(n) if self.current_class_iife_name.as_deref() == Some(&**n));
                self.emit_function_body_with_defaults(
                    parameters,
                    body,
                    *body_source_range,
                    force_multiline_empty,
                );
            }
            IRNode::LogicalOr { left, right } => {
                self.emit_node(left);
                self.write(" || ");
                // Wrap assignment expressions in parens for correctness: E || (E = {})
                let needs_parens = matches!(
                    &**right,
                    IRNode::BinaryExpr { operator, .. } if operator == "="
                );
                if needs_parens {
                    self.write("(");
                }
                self.emit_node(right);
                if needs_parens {
                    self.write(")");
                }
            }
            IRNode::LogicalAnd { left, right } => {
                self.emit_node(left);
                self.write(" && ");
                self.emit_node(right);
            }

            // Statements
            IRNode::VarDecl { name, initializer } => {
                self.write("var ");
                self.write(name);
                if let Some(init) = initializer {
                    self.write(" = ");
                    self.emit_node(init);
                }
                self.write(";");
            }
            IRNode::VarDeclList(decls) => {
                self.write("var ");
                for (i, decl) in decls.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    if let IRNode::VarDecl { name, initializer } = decl {
                        self.write(name);
                        if let Some(init) = initializer {
                            self.write(" = ");
                            self.emit_node(init);
                        }
                    } else {
                        self.emit_node(decl);
                    }
                }
                self.write(";");
            }
            IRNode::ExpressionStatement(expr) => {
                // Wrap function expressions in parens when in statement position
                // to prevent ambiguity with function declarations.
                let needs_paren = matches!(expr.as_ref(), IRNode::FunctionExpr { .. });
                if needs_paren {
                    self.write("(");
                }
                self.emit_node(expr);
                if needs_paren {
                    self.write(")");
                }
                self.write(";");
            }
            IRNode::ReturnStatement(expr) => {
                self.write("return");
                if let Some(e) = expr {
                    self.write(" ");
                    if let IRNode::ObjectLiteral {
                        properties,
                        source_range: None,
                    } = &**e
                        && Self::is_done_value_object_literal(properties)
                    {
                        self.emit_object_literal_multiline(properties);
                    } else {
                        self.emit_node(e);
                    }
                }
                self.write(";");
            }
            IRNode::IfStatement {
                condition,
                then_branch,
                else_branch,
            } => {
                self.write("if (");
                self.emit_node(condition);
                self.write(") ");
                self.emit_node(then_branch);
                if let Some(else_br) = else_branch {
                    self.write_line();
                    self.write_indent();
                    self.write("else ");
                    self.emit_node(else_br);
                }
            }
            IRNode::Block(stmts) => {
                self.emit_block(stmts);
            }
            IRNode::EmptyStatement => {
                self.write(";");
            }
            IRNode::SwitchStatement { expression, cases } => {
                self.write("switch (");
                self.emit_node(expression);
                self.write(") {");
                self.write_line();
                self.increase_indent();
                for case in cases {
                    self.emit_switch_case(case);
                }
                self.decrease_indent();
                self.write_indent();
                self.write("}");
            }
            IRNode::ForStatement {
                initializer,
                condition,
                incrementor,
                body,
            } => {
                self.write("for (");
                if let Some(init) = initializer {
                    self.emit_node(init);
                }
                self.write("; ");
                if let Some(cond) = condition {
                    self.emit_node(cond);
                }
                self.write("; ");
                if let Some(incr) = incrementor {
                    self.emit_node(incr);
                }
                self.write(") ");
                self.emit_node(body);
            }
            IRNode::WhileStatement { condition, body } => {
                self.write("while (");
                self.emit_node(condition);
                self.write(") ");
                self.emit_node(body);
            }
            IRNode::DoWhileStatement { body, condition } => {
                self.write("do ");
                self.emit_node(body);
                self.write(" while (");
                self.emit_node(condition);
                self.write(");");
            }
            IRNode::TryStatement {
                try_block,
                catch_clause,
                finally_block,
            } => {
                self.write("try ");
                self.emit_node(try_block);
                if let Some(catch) = catch_clause {
                    self.write(" catch");
                    if let Some(param) = &catch.param {
                        self.write(" (");
                        self.write(param);
                        self.write(")");
                    }
                    self.write(" ");
                    self.emit_block(&catch.body);
                }
                if let Some(finally) = finally_block {
                    self.write(" finally ");
                    self.emit_node(finally);
                }
            }
            IRNode::ThrowStatement(expr) => {
                self.write("throw ");
                self.emit_node(expr);
                self.write(";");
            }
            IRNode::BreakStatement(label) => {
                self.write("break");
                if let Some(l) = label {
                    self.write(" ");
                    self.write(l);
                }
                self.write(";");
            }
            IRNode::ContinueStatement(label) => {
                self.write("continue");
                if let Some(l) = label {
                    self.write(" ");
                    self.write(l);
                }
                self.write(";");
            }
            IRNode::LabeledStatement { label, statement } => {
                self.write(label);
                self.write(": ");
                self.emit_node(statement);
            }

            // Declarations
            IRNode::FunctionDecl {
                name,
                parameters,
                body,
                body_source_range,
                leading_comment,
            } => {
                // Emit leading JSDoc/block comment if present (e.g., constructor comment)
                if !self.remove_comments
                    && let Some(comment) = leading_comment
                {
                    self.emit_multiline_comment(comment);
                    self.write_line();
                    self.write_indent();
                }
                self.write("function ");
                self.write(name);
                self.write("(");
                self.emit_parameters(parameters);
                self.write(") ");
                let force_multiline_empty =
                    self.current_class_iife_name.as_deref() == Some(&**name);
                self.emit_function_body_with_defaults(
                    parameters,
                    body,
                    *body_source_range,
                    force_multiline_empty,
                );
                if !self.remove_comments
                    && !self.suppress_function_trailing_extraction
                    && let Some(comment) = self.extract_trailing_comment_from_function(node)
                {
                    self.write(" ");
                    self.write(&comment);
                }
            }

            // ES5 Class Transform Specific
            IRNode::ES5ClassIIFE {
                name,
                base_class,
                body,
                weakmap_decls,
                weakmap_inits,
                leading_comment,
                deferred_static_blocks,
            } => {
                // Emit WeakMap declarations if any
                if !weakmap_decls.is_empty() {
                    self.write("var ");
                    self.write(&weakmap_decls.join(", "));
                    self.write(";");
                    self.write_line();
                }
                if !self.remove_comments
                    && let Some(comment) = leading_comment
                {
                    self.write(comment);
                    self.write_line();
                }

                // var ClassName = /** @class */ (function (_super) { ... }(BaseClass));
                self.write("var ");
                self.write(name);
                if self.remove_comments {
                    self.write(" = (function (");
                } else {
                    self.write(" = /** @class */ (function (");
                }
                if base_class.is_some() {
                    self.write("_super");
                }
                self.write(") {");
                self.write_line();
                self.increase_indent();

                let prev_iife_name = self.current_class_iife_name.replace(name.to_string());

                // Emit body
                for stmt in body {
                    self.write_indent();
                    self.emit_node(stmt);
                    self.write_line();
                }
                self.current_class_iife_name = prev_iife_name;

                self.decrease_indent();
                self.write_indent();
                self.write("}(");
                if let Some(base) = base_class {
                    self.emit_node(base);
                }
                self.write("));");

                // Emit WeakMap instantiations if any
                if !weakmap_inits.is_empty() {
                    self.write_line();
                    self.write(&weakmap_inits.join(", "));
                    self.write(";");
                }

                // Emit deferred static block IIFEs after the class IIFE
                for deferred in deferred_static_blocks {
                    self.write_line();
                    self.write_indent();
                    self.emit_node(deferred);
                }
            }
            IRNode::StaticBlockIIFE { statements } => {
                // (function () { ...statements... })();
                self.write("(function () {");
                if statements.is_empty() {
                    self.write(" })();");
                } else {
                    self.write_line();
                    self.increase_indent();
                    for stmt in statements {
                        self.write_indent();
                        self.emit_node(stmt);
                        self.write_line();
                    }
                    self.decrease_indent();
                    self.write_indent();
                    self.write("})();");
                }
            }
            IRNode::ExtendsHelper { class_name } => {
                self.write_helper("__extends");
                self.write("(");
                self.write(class_name);
                self.write(", _super);");
            }
            IRNode::PrototypeMethod {
                class_name,
                method_name,
                function,
                leading_comment,
                trailing_comment,
            } => {
                // Emit leading JSDoc comment if present
                if !self.remove_comments
                    && let Some(comment) = leading_comment
                {
                    self.emit_multiline_comment(comment);
                    self.write_line();
                    self.write_indent();
                }
                self.write(class_name);
                self.write(".prototype");
                self.emit_method_name(method_name);
                self.write(" = ");
                self.emit_node(function);
                self.write(";");
                if !self.remove_comments
                    && let Some(comment) = trailing_comment
                        .clone()
                        .or_else(|| self.extract_trailing_comment_from_function(function))
                {
                    self.write(" ");
                    self.write(&comment);
                }
            }
            IRNode::StaticMethod {
                class_name,
                method_name,
                function,
                leading_comment,
                trailing_comment,
            } => {
                // Emit leading JSDoc comment if present
                if !self.remove_comments
                    && let Some(comment) = leading_comment
                {
                    self.emit_multiline_comment(comment);
                    self.write_line();
                    self.write_indent();
                }
                self.write(class_name);
                self.emit_method_name(method_name);
                self.write(" = ");
                self.emit_node(function);
                self.write(";");
                if !self.remove_comments
                    && let Some(comment) = trailing_comment
                        .clone()
                        .or_else(|| self.extract_trailing_comment_from_function(function))
                {
                    self.write(" ");
                    self.write(&comment);
                }
            }
            IRNode::DefineProperty {
                target,
                property_name,
                descriptor,
                leading_comment,
            } => {
                self.write("Object.defineProperty(");
                self.emit_node(target);
                self.write(", ");
                match property_name {
                    IRMethodName::Identifier(name) | IRMethodName::StringLiteral(name) => {
                        self.write("\"");
                        self.write(name);
                        self.write("\"");
                    }
                    IRMethodName::NumericLiteral(name) => {
                        self.write(name);
                    }
                    IRMethodName::Computed(expr) => {
                        self.emit_node(expr);
                    }
                }
                self.write(", {");
                self.write_line();
                self.increase_indent();

                // Emit leading comment inside the descriptor (before get/set)
                if !self.remove_comments
                    && let Some(comment) = leading_comment
                {
                    self.write_indent();
                    self.emit_multiline_comment(comment);
                    self.write_line();
                }

                if let Some(get) = &descriptor.get {
                    self.write_indent();
                    self.write("get: ");
                    self.emit_node(get);
                    let trailing_comment = if self.remove_comments {
                        None
                    } else {
                        descriptor
                            .trailing_comment
                            .clone()
                            .or_else(|| self.extract_trailing_comment_from_function(get))
                    };
                    if let Some(comment) = trailing_comment.as_deref() {
                        self.write(" ");
                        self.write(comment);
                        self.write_line();
                        self.write_indent();
                        self.write(",");
                    } else {
                        self.write(",");
                    }
                    self.write_line();
                }
                if let Some(set) = &descriptor.set {
                    self.write_indent();
                    self.write("set: ");
                    self.emit_node(set);
                    if !self.remove_comments {
                        if let Some(comment) = self.extract_trailing_comment_from_function(set) {
                            self.write(" ");
                            self.write(&comment);
                            self.write_line();
                            self.write_indent();
                            self.write(",");
                        } else {
                            self.write(",");
                        }
                    } else {
                        self.write(",");
                    }
                    self.write_line();
                }
                self.write_indent();
                self.write("enumerable: ");
                self.write(if descriptor.enumerable {
                    "true"
                } else {
                    "false"
                });
                self.write(",");
                self.write_line();
                self.write_indent();
                self.write("configurable: ");
                self.write(if descriptor.configurable {
                    "true"
                } else {
                    "false"
                });
                if let Some(value) = &descriptor.value {
                    self.write(",");
                    self.write_line();
                    self.write_indent();
                    self.write("writable: ");
                    self.write(if descriptor.writable { "true" } else { "false" });
                    self.write(",");
                    self.write_line();
                    self.write_indent();
                    self.write("value: ");
                    self.emit_node(value);
                }
                self.write_line();

                self.decrease_indent();
                self.write_indent();
                self.write("});");
            }

            // Async Transform Specific
            IRNode::AwaiterCall {
                this_arg,
                generator_body,
                hoisted_vars,
                promise_constructor,
            } => {
                self.write("return __awaiter(");
                self.emit_node(this_arg);
                if let Some(ctor) = promise_constructor {
                    self.write(", void 0, ");
                    self.write(ctor);
                    self.write(", function () {");
                } else {
                    self.write(", void 0, void 0, function () {");
                }
                if hoisted_vars.is_empty() {
                    // Multi-line format (matches tsc):
                    // return __awaiter(this, void 0, void 0, function () {
                    //     return __generator(this, function (_a) {
                    //         ...
                    //     });
                    // });
                    self.write_line();
                    self.increase_indent();
                    self.write_indent();
                    self.emit_node(generator_body);
                    self.decrease_indent();
                    self.write_line();
                    self.write_indent();
                    self.write("});");
                } else {
                    // Multi-line format with hoisted vars
                    self.write_line();
                    self.increase_indent();
                    for var_name in hoisted_vars {
                        self.write_indent();
                        self.write("var ");
                        self.write(var_name);
                        self.write(";");
                        self.write_line();
                    }
                    self.write_indent();
                    self.emit_node(generator_body);
                    self.decrease_indent();
                    self.write_line();
                    self.write_indent();
                    self.write("});");
                }
            }
            IRNode::GeneratorBody { has_await, cases } => {
                self.write("return ");
                self.write_helper("__generator");
                self.write("(this, function (_a) {");
                if !*has_await || cases.is_empty() {
                    // Simple body - always multi-line to match tsc
                    if cases.is_empty() {
                        self.write_line();
                        self.increase_indent();
                        self.write_indent();
                        self.write("return [2 /*return*/];");
                        self.write_line();
                        self.decrease_indent();
                        self.write_indent();
                        self.write("});");
                    } else {
                        self.write_line();
                        self.increase_indent();
                        for stmt in &cases[0].statements {
                            self.write_indent();
                            self.emit_node(stmt);
                            self.write_line();
                        }
                        self.decrease_indent();
                        self.write_indent();
                        self.write("});");
                    }
                } else {
                    // Switch/case body
                    self.write_line();
                    self.increase_indent();
                    self.write_indent();
                    self.write("switch (_a.label) {");
                    self.write_line();
                    self.increase_indent();

                    for case_item in cases {
                        self.write_indent();
                        self.write("case ");
                        self.write(&case_item.label.to_string());
                        self.write(":");
                        // tsc puts single-return-generator-op cases on one line:
                        //   case 0: return [4 /*yield*/, x];
                        if case_item.statements.len() == 1
                            && Self::is_generator_return(&case_item.statements[0])
                        {
                            self.write(" ");
                            self.emit_node(&case_item.statements[0]);
                            self.write_line();
                        } else if !case_item.statements.is_empty() {
                            self.write_line();
                            self.increase_indent();
                            for stmt in &case_item.statements {
                                self.write_indent();
                                self.emit_node(stmt);
                                self.write_line();
                            }
                            self.decrease_indent();
                        } else {
                            self.write_line();
                        }
                    }

                    self.decrease_indent();
                    self.write_indent();
                    self.write("}");
                    self.write_line();
                    self.decrease_indent();
                    self.write_indent();
                    self.write("});");
                }
            }
            IRNode::GeneratorOp {
                opcode,
                value,
                comment,
            } => {
                self.write("[");
                self.write(&opcode.to_string());
                if let Some(cmt) = comment {
                    self.write(" /*");
                    self.write(cmt);
                    self.write("*/");
                }
                if let Some(val) = value {
                    self.write(", ");
                    self.emit_node(val);
                }
                self.write("]");
            }
            IRNode::GeneratorSent => {
                self.write("_a.sent()");
            }
            IRNode::GeneratorLabel => {
                self.write("_a.label");
            }

            IRNode::IfBreak {
                condition,
                target_label,
            } => {
                self.write("if (");
                self.emit_node(condition);
                self.write(") return [3 /*break*/, ");
                self.write(&target_label.to_string());
                self.write("];");
            }

            // Private Field Helpers
            IRNode::PrivateFieldGet {
                receiver,
                weakmap_name,
            } => {
                self.write_helper("__classPrivateFieldGet");
                self.write("(");
                self.emit_node(receiver);
                self.write(", ");
                self.write(weakmap_name);
                self.write(", \"f\")");
            }
            IRNode::PrivateFieldSet {
                receiver,
                weakmap_name,
                value,
            } => {
                self.write_helper("__classPrivateFieldSet");
                self.write("(");
                self.emit_node(receiver);
                self.write(", ");
                self.write(weakmap_name);
                self.write(", ");
                self.emit_node(value);
                self.write(", \"f\")");
            }
            IRNode::PrivateFieldIn { weakmap_name, obj } => {
                self.write_helper("__classPrivateFieldIn");
                self.write("(");
                self.write(weakmap_name);
                self.write(", ");
                self.emit_node(obj);
                self.write(")");
            }
            IRNode::WeakMapSet {
                weakmap_name,
                key,
                value,
            } => {
                self.write(weakmap_name);
                self.write(".set(");
                self.emit_node(key);
                self.write(", ");
                self.emit_node(value);
                self.write(")");
            }

            // Special
            IRNode::Raw(s) => {
                // Comments stored as Raw nodes bypass IRNode::Comment guards.
                // Detect and suppress them when removeComments is enabled.
                if self.remove_comments {
                    let t = s.trim_start();
                    if t.starts_with("//") || t.starts_with("/*") {
                        // Skip comment-like Raw node
                    } else {
                        self.write(s);
                    }
                } else {
                    self.write(s);
                }
            }
            IRNode::Comment { text, is_block } => {
                if !self.remove_comments {
                    if *is_block {
                        self.write("/*");
                        self.write(text);
                        self.write("*/");
                    } else {
                        self.write("// ");
                        self.write(text);
                    }
                }
            }
            IRNode::TrailingComment(text) => {
                // When encountered outside the body loop, just emit the text.
                // Inside the body loop, this is consumed by peek-ahead logic.
                if !self.remove_comments {
                    self.write(" ");
                    self.write(text);
                }
            }
            IRNode::Sequence(nodes) => {
                let mut i = 0;
                while i < nodes.len() {
                    if matches!(&nodes[i], IRNode::TrailingComment(_)) {
                        i += 1;
                        continue;
                    }
                    // Skip comment nodes when removeComments is enabled
                    if self.remove_comments
                        && (matches!(&nodes[i], IRNode::Comment { .. })
                            || matches!(&nodes[i], IRNode::Raw(s) if s.trim_start().starts_with("//") || s.trim_start().starts_with("/*")))
                    {
                        i += 1;
                        continue;
                    }
                    if i + 1 < nodes.len()
                        && let Some((enum_name, members, namespace)) =
                            Self::enum_with_matching_namespace_export(&nodes[i], &nodes[i + 1])
                    {
                        self.emit_namespace_bound_enum_iife(enum_name, members, namespace);
                        i += 2;
                        if i < nodes.len() {
                            self.write_line();
                            if Self::should_indent_sequence_child(&nodes[i]) {
                                self.write_indent();
                            }
                        }
                        continue;
                    }

                    let suppress_for_this_node = i + 1 < nodes.len()
                        && matches!(&nodes[i], IRNode::FunctionDecl { .. })
                        && matches!(&nodes[i + 1], IRNode::TrailingComment(_));
                    let prev_suppress = self.suppress_function_trailing_extraction;
                    self.suppress_function_trailing_extraction = suppress_for_this_node;
                    self.emit_node(&nodes[i]);
                    self.suppress_function_trailing_extraction = prev_suppress;
                    if i + 1 < nodes.len()
                        && let IRNode::TrailingComment(text) = &nodes[i + 1]
                    {
                        if !self.remove_comments {
                            self.write(" ");
                            self.write(text);
                        }
                        i += 1;
                    }

                    if i < nodes.len() - 1 {
                        self.write_line();
                        if Self::should_indent_sequence_child(&nodes[i + 1]) {
                            self.write_indent();
                        }
                    }
                    i += 1;
                }
            }
            IRNode::ASTRef(idx) => {
                // Check if this node has a transform directive that we should apply
                if let Some(arena) = self.arena
                    && let Some(node) = arena.get(*idx)
                {
                    // For variable statements, directives are often attached to the nested
                    // declaration-list node. Delegate full statement emission to Printer so
                    // ES5 variable-list transforms still apply while preserving comments.
                    if node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                        && let Some(ref transforms) = self.transforms
                        && let Some(var_stmt) = arena.get_variable(node)
                    {
                        let has_es5_var_list_directive =
                            var_stmt.declarations.nodes.iter().any(|decl_idx| {
                                matches!(
                                    transforms.get(*decl_idx),
                                    Some(
                                        crate::context::transform::TransformDirective::ES5VariableDeclarationList { .. }
                                    )
                                )
                            });
                        if has_es5_var_list_directive {
                            let mut printer = AstPrinter::with_transforms_and_options(
                                arena,
                                transforms.clone(),
                                PrinterOptions::default(),
                            );
                            if let Some(source_text) = self.source_text {
                                printer.set_source_text(source_text);
                            }
                            printer.emit(*idx);
                            self.write(printer.get_output());
                            return;
                        }
                    }

                    if node.kind == syntax_kind_ext::EXPRESSION_STATEMENT
                        && let Some(ref transforms) = self.transforms
                        && let Some(stmt) = arena.get_expression_statement(node)
                        && let Some(expr_node) = arena.get(stmt.expression)
                    {
                        let target_expr =
                            if expr_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                                arena
                                    .get_parenthesized(expr_node)
                                    .map(|p| p.expression)
                                    .unwrap_or(stmt.expression)
                            } else {
                                stmt.expression
                            };
                        let is_destructuring_assignment = arena
                            .get(target_expr)
                            .is_some_and(|n| n.kind == syntax_kind_ext::BINARY_EXPRESSION)
                            && arena
                                .get(target_expr)
                                .and_then(|n| arena.get_binary_expr(n))
                                .is_some_and(|bin| {
                                    bin.operator_token
                                        == tsz_scanner::SyntaxKind::EqualsToken as u16
                                        && arena.get(bin.left).is_some_and(|left| {
                                            left.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                                || left.kind
                                                    == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                                        })
                                });
                        if is_destructuring_assignment {
                            let mut printer = AstPrinter::with_transforms_and_options(
                                arena,
                                transforms.clone(),
                                PrinterOptions {
                                    target: crate::emitter::ScriptTarget::ES5,
                                    ..PrinterOptions::default()
                                },
                            );
                            if let Some(source_text) = self.source_text {
                                printer.set_source_text(source_text);
                            }
                            printer.emit(*idx);
                            self.write(printer.get_output());
                            return;
                        }
                    }

                    // Get the directive for this node (clone to avoid borrow issues)
                    let directive = self.transforms.as_ref().and_then(|t| t.get(*idx).cloned());

                    if let Some(directive) = directive {
                        use tsz_parser::parser::syntax_kind_ext;

                        // Handle ES5ArrowFunction directive
                        if matches!(
                            directive,
                            crate::context::transform::TransformDirective::ES5ArrowFunction { .. }
                        ) && node.kind == syntax_kind_ext::ARROW_FUNCTION
                            && let Some(func_data) = arena.get_function(node)
                        {
                            // Async arrows need __awaiter/__generator lowering.
                            // Delegate to the full AstPrinter which has the
                            // emit_arrow_function_es5 → emit_async_arrow_es5_inline path.
                            if func_data.is_async
                                && let Some(ref transforms) = self.transforms
                            {
                                let mut printer = AstPrinter::with_transforms_and_options(
                                    arena,
                                    transforms.clone(),
                                    PrinterOptions {
                                        target: crate::emitter::ScriptTarget::ES5,
                                        ..PrinterOptions::default()
                                    },
                                );
                                if let Some(source_text) = self.source_text {
                                    printer.set_source_text(source_text);
                                }
                                printer.emit_expression(*idx);
                                self.write(printer.get_output());
                                return;
                            }
                            self.emit_arrow_function_es5_with_flags(arena, func_data);
                            return;
                        }

                        // Handle SubstituteThis directive
                        if let crate::context::transform::TransformDirective::SubstituteThis {
                            ref capture_name,
                        } = directive
                            && let Some(_ident) = arena.get_identifier(node)
                        {
                            self.write(capture_name);
                            return;
                        }

                        // Handle SubstituteArguments directive
                        if matches!(
                            directive,
                            crate::context::transform::TransformDirective::SubstituteArguments
                        ) && let Some(_ident) = arena.get_identifier(node)
                        {
                            self.write("arguments");
                            return;
                        }

                        // Handle ES5TemplateLiteral directive — downlevel template literals
                        // to string concatenation using .concat() calls
                        if matches!(
                            directive,
                            crate::context::transform::TransformDirective::ES5TemplateLiteral { .. }
                        ) && let Some(ref transforms) = self.transforms
                        {
                            let mut printer = AstPrinter::with_transforms_and_options(
                                arena,
                                transforms.clone(),
                                PrinterOptions {
                                    target: crate::emitter::ScriptTarget::ES5,
                                    ..PrinterOptions::default()
                                },
                            );
                            if let Some(source_text) = self.source_text {
                                printer.set_source_text(source_text);
                            }
                            printer.emit(*idx);
                            self.write(printer.get_output());
                            return;
                        }

                        // Note: For other directive types, fall through to source text copy
                        // This is intentional - we only handle directives that are ready
                    }
                }

                // When transforms exist, delegate to AstPrinter so that nested
                // directives (e.g. ES5 template literal downleveling, this/arguments
                // substitution) inside this subtree are applied correctly.
                if let Some(arena) = self.arena
                    && let Some(ref transforms) = self.transforms
                    && !transforms.is_empty()
                {
                    let mut printer = AstPrinter::with_transforms_and_options(
                        arena,
                        transforms.clone(),
                        PrinterOptions {
                            target: crate::emitter::ScriptTarget::ES5,
                            ..PrinterOptions::default()
                        },
                    );
                    if let Some(source_text) = self.source_text {
                        printer.set_source_text(source_text);
                    }
                    printer.emit(*idx);
                    let output = printer.get_output().to_string();
                    let trimmed = output.trim();
                    if !trimmed.is_empty() {
                        self.write(trimmed);
                        return;
                    }
                }

                // Emit AST node by using its source text.
                // For expressions, just emit the trimmed text directly.
                // For statements, we need to find the statement end.
                if let Some(arena) = self.arena
                    && let Some(text) = self.source_text
                    && let Some(node) = arena.get(*idx)
                {
                    let start = node.pos as usize;
                    let end = std::cmp::min(node.end as usize, text.len());
                    if start < end {
                        let raw = &text[start..end];
                        // Trim both leading and trailing whitespace for expressions
                        let trimmed = raw.trim();
                        if !trimmed.is_empty() {
                            self.write(trimmed);
                            return;
                        }
                    }
                }
                // Fallback: emit placeholder
                self.write("undefined");
            }

            IRNode::ASTRefRange(idx, max_end) => {
                // Like ASTRef but with a constrained end position.
                // Used when a statement's node.end extends into a parent block's closing brace.
                if let Some(arena) = self.arena
                    && let Some(text) = self.source_text
                    && let Some(node) = arena.get(*idx)
                {
                    let start = node.pos as usize;
                    let end = std::cmp::min(*max_end as usize, text.len());
                    let end = std::cmp::min(end, node.end as usize);
                    if start < end {
                        let raw = &text[start..end];
                        let trimmed = raw.trim();
                        if !trimmed.is_empty() {
                            self.write(trimmed);
                            return;
                        }
                    }
                }
                self.write("undefined");
            }

            // CommonJS Module Transform Specific
            IRNode::UseStrict => {
                self.write("\"use strict\";");
            }
            IRNode::EsesModuleMarker => {
                self.write("Object.defineProperty(exports, \"__esModule\", { value: true });");
            }
            IRNode::ExportInit { name } => {
                self.write("exports.");
                self.write(name);
                self.write(" = void 0;");
            }
            IRNode::RequireStatement {
                var_name,
                module_spec,
            } => {
                self.write("var ");
                self.write(var_name);
                self.write(" = require(\"");
                self.write(module_spec);
                self.write("\");");
            }
            IRNode::DefaultImport {
                var_name,
                module_var,
            } => {
                self.write("var ");
                self.write(var_name);
                self.write(" = ");
                self.write(module_var);
                self.write(".default;");
            }
            IRNode::NamespaceImport {
                var_name,
                module_var,
            } => {
                self.write("var ");
                self.write(var_name);
                self.write(" = __importStar(");
                self.write(module_var);
                self.write(");");
            }
            IRNode::NamedImport {
                var_name,
                module_var,
                import_name,
            } => {
                self.write("var ");
                self.write(var_name);
                self.write(" = ");
                self.write(module_var);
                self.write(".");
                self.write(import_name);
                self.write(";");
            }
            IRNode::ExportAssignment { name } => {
                self.write("exports.");
                self.write(name);
                self.write(" = ");
                self.write(name);
                self.write(";");
            }
            IRNode::ReExportProperty {
                export_name,
                module_var,
                import_name,
            } => {
                self.write("Object.defineProperty(exports, \"");
                self.write(export_name);
                self.write("\", { enumerable: true, get: function () { return ");
                self.write(module_var);
                self.write(".");
                self.write(import_name);
                self.write("; } });");
            }

            // Enum Transform Specific
            IRNode::EnumIIFE {
                name,
                members,
                namespace_export,
            } => {
                if let Some(ns) = namespace_export {
                    self.emit_namespace_bound_enum_iife(name, members, ns);
                } else {
                    // var E; (top-level) or let E; (inside namespace/function body at ES2015+)
                    let keyword = if self.in_namespace_iife_body && !self.target_es5 {
                        "let"
                    } else {
                        "var"
                    };
                    self.write(keyword);
                    self.write(" ");
                    self.write(name);
                    self.write(";");
                    self.write_line();
                    self.write_indent();
                    self.write("(function (");
                    self.write(name);
                    self.write(") {");
                    self.write_line();
                    self.increase_indent();

                    // Emit members
                    for member in members {
                        self.write_indent();
                        self.emit_enum_member(name, member);
                        self.write_line();
                    }

                    self.decrease_indent();
                    self.write_indent();
                    self.write("})(");
                    self.write(name);
                    self.write(" || (");
                    self.write(name);
                    self.write(" = {}));");
                }
            }

            // Namespace Transform Specific
            IRNode::NamespaceIIFE {
                name: _name,
                name_parts,
                body,
                is_exported,
                attach_to_exports,
                should_declare_var,
                parent_name,
                param_name,
                skip_sequence_indent: _,
            } => {
                self.emit_namespace_iife(
                    name_parts,
                    0,
                    body,
                    NamespaceIifeContext {
                        is_exported: *is_exported,
                        attach_to_exports: *attach_to_exports,
                        should_declare_var: *should_declare_var,
                        parent_name: parent_name.as_deref(),
                        param_name: param_name.as_deref(),
                    },
                );
            }
            IRNode::NamespaceExport {
                namespace,
                name,
                value,
            } => {
                self.write(namespace);
                self.write(".");
                self.write(name);
                self.write(" = ");
                self.emit_node(value);
                self.write(";");
            }
        }
    }

    fn emit_enum_member(&mut self, enum_name: &str, member: &EnumMember) {
        self.write(enum_name);

        match &member.value {
            EnumMemberValue::Auto(value) | EnumMemberValue::Numeric(value) => {
                // Numeric enum with reverse mapping: E[E["A"] = 0] = "A";
                self.write("[");
                self.write(enum_name);
                self.write("[\"");
                self.write(&member.name);
                self.write("\"] = ");
                self.write(&value.to_string());
                self.write("] = \"");
                self.write(&member.name);
                self.write("\";");
            }
            EnumMemberValue::String(s) => {
                // String enum, no reverse mapping: E["A"] = "val";
                self.write("[\"");
                self.write(&member.name);
                self.write("\"] = \"");
                self.write_escaped(s);
                self.write("\";");
            }
            EnumMemberValue::Computed(expr) => {
                // Computed enum with reverse mapping
                self.write("[");
                self.write(enum_name);
                self.write("[\"");
                self.write(&member.name);
                self.write("\"] = ");
                self.emit_node(expr);
                self.write("] = \"");
                self.write(&member.name);
                self.write("\";");
            }
        }
    }

    fn emit_namespace_iife(
        &mut self,
        name_parts: &[Cow<'static, str>],
        index: usize,
        body: &[IRNode],
        context: NamespaceIifeContext<'_>,
    ) {
        let current_name = &name_parts[index];
        let is_last = index == name_parts.len() - 1;
        // Use renamed parameter name only at the innermost (last) level for collision avoidance.
        // Outer levels of qualified names (A.B.C) always use their original name.
        let iife_param = if is_last {
            context.param_name.unwrap_or(current_name)
        } else {
            current_name
        };

        // Emit var/let declaration only for the outermost namespace and if flag is true.
        // Inside a namespace IIFE body, use `let` (ES2015+ semantics for nested namespaces).
        // At the outermost level, use `var` (needed for declaration merging across files).
        if index == 0 && context.should_declare_var {
            let decl_keyword = if self.in_namespace_iife_body && !self.target_es5 {
                "let"
            } else {
                "var"
            };
            self.write(decl_keyword);
            self.write(" ");
            self.write(current_name);
            self.write(";");
            self.write_line();
            // Need indent after the newline from var declaration
            self.write_indent();
        }
        // When should_declare_var is false, the caller already wrote the indent
        // (the parent namespace body loop calls write_indent before emit_node).

        // Open IIFE: (function (name) {
        self.write("(function (");
        self.write(iife_param);
        self.write(") {");
        self.write_line();
        self.increase_indent();

        if is_last {
            // Emit body with trailing comment peek-ahead.
            // Set in_namespace_iife_body so nested namespace declarations use `let`.
            let prev_in_ns_body = self.in_namespace_iife_body;
            self.in_namespace_iife_body = true;
            let mut i = 0;
            while i < body.len() {
                // Skip standalone TrailingComment nodes (consumed by peek-ahead)
                if matches!(&body[i], IRNode::TrailingComment(_)) {
                    i += 1;
                    continue;
                }
                // Skip comment nodes when removeComments is enabled
                if self.remove_comments
                    && (matches!(&body[i], IRNode::Comment { .. })
                        || matches!(&body[i], IRNode::Raw(s) if s.trim_start().starts_with("//") || s.trim_start().starts_with("/*")))
                {
                    i += 1;
                    continue;
                }

                if Self::is_noop_statement(&body[i]) {
                    i += 1;
                    continue;
                }

                self.write_indent();
                let suppress_for_this_node = i + 1 < body.len()
                    && matches!(&body[i], IRNode::FunctionDecl { .. })
                    && matches!(&body[i + 1], IRNode::TrailingComment(_));
                let prev_suppress = self.suppress_function_trailing_extraction;
                self.suppress_function_trailing_extraction = suppress_for_this_node;
                self.emit_node(&body[i]);
                self.suppress_function_trailing_extraction = prev_suppress;
                // Peek ahead for trailing comment
                if i + 1 < body.len()
                    && let IRNode::TrailingComment(text) = &body[i + 1]
                {
                    if !self.remove_comments {
                        self.write(" ");
                        self.write(text);
                    }
                    i += 1; // consume the trailing comment
                }
                self.write_line();
                i += 1;
            }
            // Restore in_namespace_iife_body after emitting this namespace's body
            self.in_namespace_iife_body = prev_in_ns_body;
        } else {
            // Emit var/let declaration for nested namespace (dotted namespaces: A.B.C)
            // Use `let` inside namespace bodies (ES2015+ semantics).
            let next_name = &name_parts[index + 1];
            self.write_indent();
            let nested_decl_keyword = if self.in_namespace_iife_body && !self.target_es5 {
                "let"
            } else {
                "var"
            };
            self.write(nested_decl_keyword);
            self.write(" ");
            self.write(next_name);
            self.write(";");
            self.write_line();
            // Recurse for nested namespace (inner levels use name_parts[index-1] as parent).
            // Write indent since we're on a new line after "var Y;\n".
            self.write_indent();
            self.emit_namespace_iife(
                name_parts,
                index + 1,
                body,
                NamespaceIifeContext {
                    is_exported: context.is_exported,
                    attach_to_exports: context.attach_to_exports,
                    should_declare_var: true,
                    parent_name: None,
                    param_name: context.param_name,
                },
            );
            self.write_line();
        }

        self.decrease_indent();
        self.write_indent();
        self.write("})(");

        // Argument: emit the IIFE argument binding
        if index == 0 {
            if let Some(parent) = context.parent_name {
                // Nested namespace with parent: Name = Parent.Name || (Parent.Name = {})
                self.write(current_name);
                self.write(" = ");
                self.write(parent);
                self.write(".");
                self.write(current_name);
                self.write(" || (");
                self.write(parent);
                self.write(".");
                self.write(current_name);
                self.write(" = {})");
            } else if context.is_exported && context.attach_to_exports {
                self.write(current_name);
                self.write(" || (exports.");
                self.write(current_name);
                self.write(" = ");
                self.write(current_name);
                self.write(" = {})");
            } else {
                self.write(current_name);
                self.write(" || (");
                self.write(current_name);
                self.write(" = {})");
            }
        } else {
            // Qualified name parts (A.B.C): Name = Parent.Name || (Parent.Name = {})
            let parent = &name_parts[index - 1];
            self.write(current_name);
            self.write(" = ");
            self.write(parent);
            self.write(".");
            self.write(current_name);
            self.write(" || (");
            self.write(parent);
            self.write(".");
            self.write(current_name);
            self.write(" = {})");
        }

        self.write(");");
    }

    fn emit_block(&mut self, stmts: &[IRNode]) {
        if stmts.is_empty() {
            self.write("{ }");
            return;
        }

        self.write("{");
        self.write_line();
        self.increase_indent();

        for stmt in stmts {
            self.write_indent();
            self.emit_node(stmt);
            self.write_line();
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
    }
}

#[cfg(test)]
#[path = "../../tests/ir_printer.rs"]
mod tests;
