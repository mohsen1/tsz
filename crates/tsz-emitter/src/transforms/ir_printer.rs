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

use std::fmt::Write;

use crate::transform_context::TransformContext;
use crate::transforms::ir::*;
use tsz_parser::parser::base::NodeIndex;
use tsz_parser::parser::node::{Node, NodeArena};
use tsz_parser::syntax_kind_ext;

/// IR Printer - converts IR nodes to JavaScript strings
pub struct IRPrinter<'a> {
    output: String,
    indent_level: u32,
    indent_str: &'static str,
    /// Optional arena for handling ASTRef nodes
    arena: Option<&'a NodeArena>,
    /// Source text for emitting ASTRef nodes
    source_text: Option<&'a str>,
    /// Optional transform directives for ASTRef nodes
    transforms: Option<TransformContext>,
    /// Avoid duplicate trailing comments when a sequence explicitly carries one.
    suppress_function_trailing_extraction: bool,
    /// Forces empty function bodies to emit in multi-line style.
    force_multiline_empty_function_body: bool,
}

impl<'a> IRPrinter<'a> {
    fn enum_with_matching_namespace_export<'b>(
        first: &'b IRNode,
        second: &'b IRNode,
    ) -> Option<(&'b str, &'b Vec<EnumMember>, &'b str)> {
        let IRNode::EnumIIFE { name, members } = first else {
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
        if export_name == name && identifier_name == name {
            Some((name.as_str(), members, namespace.as_str()))
        } else {
            None
        }
    }

    fn emit_namespace_bound_enum_iife(
        &mut self,
        enum_name: &str,
        members: &[EnumMember],
        namespace: &str,
    ) {
        self.write("var ");
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
            } => (*body_start, *body_end),
            IRNode::FunctionDecl {
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
        for i in start..end {
            if bytes[i] == b'}'
                && let Some(comment) =
                    crate::emitter::get_trailing_comment_ranges(source_text, i + 1).first()
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
            force_multiline_empty_function_body: false,
        }
    }

    /// Create an IR printer with an arena for ASTRef handling
    pub fn with_arena(arena: &'a NodeArena) -> Self {
        Self {
            output: String::with_capacity(4096),
            indent_level: 0,
            indent_str: "    ",
            arena: Some(arena),
            source_text: None,
            transforms: None,
            suppress_function_trailing_extraction: false,
            force_multiline_empty_function_body: false,
        }
    }

    /// Create an IR printer with both arena and source text for ASTRef emission
    pub fn with_arena_and_source(arena: &'a NodeArena, source_text: &'a str) -> Self {
        Self {
            output: String::with_capacity(4096),
            indent_level: 0,
            indent_str: "    ",
            arena: Some(arena),
            source_text: Some(source_text),
            transforms: None,
            suppress_function_trailing_extraction: false,
            force_multiline_empty_function_body: false,
        }
    }

    /// Set transform directives for ASTRef emission
    pub fn set_transforms(&mut self, transforms: TransformContext) {
        self.transforms = Some(transforms);
    }

    /// Set the source text for ASTRef emission
    pub fn set_source_text(&mut self, text: &'a str) {
        self.source_text = Some(text);
    }

    /// Set the indentation level
    pub fn set_indent_level(&mut self, level: u32) {
        self.indent_level = level;
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

    fn emit_node(&mut self, node: &IRNode) {
        match node {
            // Literals
            IRNode::NumericLiteral(n) => self.write(n),
            IRNode::StringLiteral(s) => {
                self.write("\"");
                self.write_escaped(s);
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
                }
                self.emit_node(callee);
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
                self.write(".");
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
                    // Default to single-line when no source info.
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

                if !has_defaults
                    && should_emit_single_line
                    && body.len() == 1
                    && let IRNode::ReturnStatement(Some(expr)) = &body[0]
                {
                    self.write("{ return ");
                    self.emit_node(expr);
                    self.write("; }");
                    return;
                }
                self.emit_function_body_with_defaults(parameters, body, *body_source_range);
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
                self.emit_node(expr);
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
            } => {
                self.write("function ");
                self.write(name);
                self.write("(");
                self.emit_parameters(parameters);
                self.write(") ");
                self.emit_function_body_with_defaults(parameters, body, *body_source_range);
                if !self.suppress_function_trailing_extraction
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
            } => {
                // Emit WeakMap declarations if any
                if !weakmap_decls.is_empty() {
                    self.write("var ");
                    self.write(&weakmap_decls.join(", "));
                    self.write(";");
                    self.write_line();
                }

                // var ClassName = /** @class */ (function (_super) { ... }(BaseClass));
                self.write("var ");
                self.write(name);
                self.write(" = /** @class */ (function (");
                if base_class.is_some() {
                    self.write("_super");
                }
                self.write(") {");
                self.write_line();
                self.increase_indent();

                // Emit body
                for stmt in body {
                    let prev_force_multiline = self.force_multiline_empty_function_body;
                    self.force_multiline_empty_function_body = matches!(
                        stmt,
                        IRNode::FunctionDecl { name: fn_name, .. } if fn_name == name
                    );
                    self.write_indent();
                    self.emit_node(stmt);
                    self.write_line();
                    self.force_multiline_empty_function_body = prev_force_multiline;
                }

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
            }
            IRNode::ExtendsHelper { class_name } => {
                self.write("__extends(");
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
                if let Some(comment) = leading_comment {
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
                if let Some(comment) = trailing_comment
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
                if let Some(comment) = leading_comment {
                    self.emit_multiline_comment(comment);
                    self.write_line();
                    self.write_indent();
                }
                self.write(class_name);
                self.emit_method_name(method_name);
                self.write(" = ");
                self.emit_node(function);
                self.write(";");
                if let Some(comment) = trailing_comment
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

                if let Some(get) = &descriptor.get {
                    self.write_indent();
                    self.write("get: ");
                    self.emit_node(get);
                    self.write(",");
                    self.write_line();
                }
                if let Some(set) = &descriptor.set {
                    self.write_indent();
                    self.write("set: ");
                    self.emit_node(set);
                    self.write(",");
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
                self.write_line();

                self.decrease_indent();
                self.write_indent();
                self.write("});");
            }

            // Async Transform Specific
            IRNode::AwaiterCall {
                this_arg,
                generator_body,
            } => {
                self.write("return __awaiter(");
                self.emit_node(this_arg);
                self.write(", void 0, void 0, function () {");
                self.write_line();
                self.increase_indent();
                self.write_indent();
                self.emit_node(generator_body);
                self.decrease_indent();
                self.write_line();
                self.write_indent();
                self.write("});");
            }
            IRNode::GeneratorBody { has_await, cases } => {
                self.write("return __generator(this, function (_a) {");
                if !*has_await || cases.is_empty() {
                    // Simple body
                    if cases.is_empty() {
                        self.write(" return [2 /*return*/]; });");
                    } else if cases.len() == 1 && cases[0].statements.len() == 1 {
                        // Single statement, inline
                        self.write(" ");
                        self.emit_node(&cases[0].statements[0]);
                        self.write(" });");
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

                    for case in cases {
                        self.write_indent();
                        self.write("case ");
                        self.write(&case.label.to_string());
                        self.write(":");
                        if !case.statements.is_empty() {
                            self.write_line();
                            self.increase_indent();
                            for stmt in &case.statements {
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

            // Private Field Helpers
            IRNode::PrivateFieldGet {
                receiver,
                weakmap_name,
            } => {
                self.write("__classPrivateFieldGet(");
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
                self.write("__classPrivateFieldSet(");
                self.emit_node(receiver);
                self.write(", ");
                self.write(weakmap_name);
                self.write(", ");
                self.emit_node(value);
                self.write(", \"f\")");
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
                self.write(s);
            }
            IRNode::Comment { text, is_block } => {
                if *is_block {
                    self.write("/*");
                    self.write(text);
                    self.write("*/");
                } else {
                    self.write("// ");
                    self.write(text);
                }
            }
            IRNode::TrailingComment(text) => {
                // When encountered outside the body loop, just emit the text.
                // Inside the body loop, this is consumed by peek-ahead logic.
                self.write(" ");
                self.write(text);
            }
            IRNode::Sequence(nodes) => {
                let mut i = 0;
                while i < nodes.len() {
                    if matches!(&nodes[i], IRNode::TrailingComment(_)) {
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
                        self.write(" ");
                        self.write(text);
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
                if let Some(arena) = self.arena {
                    if let Some(node) = arena.get(*idx) {
                        // Get the directive for this node (clone to avoid borrow issues)
                        let directive = self.transforms.as_ref().and_then(|t| t.get(*idx).cloned());

                        if let Some(directive) = directive {
                            use tsz_parser::parser::syntax_kind_ext;

                            // Handle ES5ArrowFunction directive
                            if matches!(
                                directive,
                                crate::transform_context::TransformDirective::ES5ArrowFunction { .. }
                            ) && node.kind == syntax_kind_ext::ARROW_FUNCTION
                            {
                                if let Some(func_data) = arena.get_function(node) {
                                    // Extract flags from directive before mutable borrow
                                    let (captures_this, captures_arguments, class_alias) = match &directive {
                                        crate::transform_context::TransformDirective::ES5ArrowFunction {
                                            captures_this,
                                            captures_arguments,
                                            class_alias,
                                            ..
                                        } => (*captures_this, *captures_arguments, class_alias.as_deref().map(|s| s.to_string())),
                                        _ => (false, false, None),
                                    };

                                    self.emit_arrow_function_es5_with_flags(
                                        arena,
                                        node,
                                        func_data,
                                        *idx,
                                        captures_this,
                                        captures_arguments,
                                        class_alias,
                                    );
                                    return;
                                }
                            }

                            // Handle SubstituteThis directive
                            if let crate::transform_context::TransformDirective::SubstituteThis {
                                ref capture_name,
                            } = directive
                            {
                                if let Some(_ident) = arena.get_identifier(node) {
                                    self.write(capture_name);
                                    return;
                                }
                            }

                            // Handle SubstituteArguments directive
                            if matches!(
                                directive,
                                crate::transform_context::TransformDirective::SubstituteArguments
                            ) {
                                if let Some(_ident) = arena.get_identifier(node) {
                                    self.write("_arguments");
                                    return;
                                }
                            }

                            // Note: For other directive types, fall through to source text copy
                            // This is intentional - we only handle directives that are ready
                        }
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
            IRNode::EnumIIFE { name, members } => {
                // var E;
                self.write("var ");
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
                    *is_exported,
                    *attach_to_exports,
                    *should_declare_var,
                    parent_name.as_deref(),
                    param_name.as_deref(),
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
        name_parts: &[String],
        index: usize,
        body: &[IRNode],
        is_exported: bool,
        attach_to_exports: bool,
        should_declare_var: bool,
        parent_name: Option<&str>,
        param_name: Option<&str>,
    ) {
        let current_name = &name_parts[index];
        let is_last = index == name_parts.len() - 1;
        // Use renamed parameter name only at the innermost (last) level for collision avoidance.
        // Outer levels of qualified names (A.B.C) always use their original name.
        let iife_param = if is_last {
            param_name.unwrap_or(current_name.as_str())
        } else {
            current_name.as_str()
        };

        // Emit var declaration only for the outermost namespace and if flag is true
        if index == 0 && should_declare_var {
            self.write("var ");
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
            // Emit body with trailing comment peek-ahead
            let mut i = 0;
            while i < body.len() {
                // Skip standalone TrailingComment nodes (consumed by peek-ahead)
                if matches!(&body[i], IRNode::TrailingComment(_)) {
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
                if i + 1 < body.len() {
                    if let IRNode::TrailingComment(text) = &body[i + 1] {
                        self.write(" ");
                        self.write(text);
                        i += 1; // consume the trailing comment
                    }
                }
                self.write_line();
                i += 1;
            }
        } else {
            // Emit var declaration for nested namespace
            let next_name = &name_parts[index + 1];
            self.write_indent();
            self.write("var ");
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
                is_exported,
                attach_to_exports,
                true,
                None,
                param_name,
            );
            self.write_line();
        }

        self.decrease_indent();
        self.write_indent();
        self.write("})(");

        // Argument: emit the IIFE argument binding
        if index == 0 {
            if let Some(parent) = parent_name {
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
            } else if is_exported && attach_to_exports {
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

    /// Check if a body source range represents a single-line block in the source text.
    /// Uses brace depth counting to find the matching `}` and skips leading trivia.
    /// Check if a source range is on a single line (for object literals, etc.)
    fn is_single_line_range(&self, pos: u32, end: u32) -> bool {
        self.source_text
            .map(|text| {
                let start = pos as usize;
                let end = std::cmp::min(end as usize, text.len());
                if start < end {
                    let slice = &text[start..end];
                    !slice.contains('\n')
                } else {
                    true // Empty range is considered single-line
                }
            })
            .unwrap_or(true) // Default to single-line if no source text
    }

    fn is_body_source_single_line(&self, body_source_range: Option<(u32, u32)>) -> bool {
        body_source_range
            .and_then(|(pos, end)| {
                self.source_text.map(|text| {
                    let start = pos as usize;
                    let end = std::cmp::min(end as usize, text.len());
                    if start < end {
                        let slice = &text[start..end];
                        if let Some(open) = slice.find('{') {
                            let mut depth = 1;
                            for (i, ch) in slice[open + 1..].char_indices() {
                                match ch {
                                    '{' => depth += 1,
                                    '}' => {
                                        depth -= 1;
                                        if depth == 0 {
                                            let inner = &slice[open..open + 1 + i + 1];
                                            return !inner.contains('\n');
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        !slice.contains('\n')
                    } else {
                        false
                    }
                })
            })
            .unwrap_or(false)
    }

    /// Emit function body with default parameter checks prepended (ES5 style)
    fn emit_function_body_with_defaults(
        &mut self,
        params: &[IRParam],
        body: &[IRNode],
        body_source_range: Option<(u32, u32)>,
    ) {
        // Check if any params have defaults
        let has_defaults = params.iter().any(|p| p.default_value.is_some());

        // Check if the body was single-line in the source
        let is_body_source_single_line = self.is_body_source_single_line(body_source_range);

        // Empty body with no defaults: emit as single-line { } if source was single-line
        if !has_defaults && body.is_empty() {
            if is_body_source_single_line && !self.force_multiline_empty_function_body {
                self.write("{ }");
            } else {
                self.write("{");
                self.write_line();
                self.write_indent();
                self.write("}");
            }
            return;
        }

        // Single statement with no defaults: emit as single-line if source was single-line,
        // unless caller forced multiline style (used for class constructors in ES5 class IIFEs).
        if !has_defaults
            && body.len() == 1
            && is_body_source_single_line
            && !self.force_multiline_empty_function_body
        {
            self.write("{ ");
            self.emit_node(&body[0]);
            self.write(" }");
            return;
        }

        // Multi-line body (either has defaults, multiple statements, or wasn't single-line in source)
        self.write("{");
        self.write_line();
        self.increase_indent();

        // Emit default parameter checks: if (param === void 0) { param = default; }
        for param in params {
            if let Some(default) = &param.default_value {
                self.write_indent();
                self.write("if (");
                self.write(&param.name);
                self.write(" === void 0) { ");
                self.write(&param.name);
                self.write(" = ");
                self.emit_node(default);
                self.write("; }");
                self.write_line();
            }
        }

        // Emit the rest of the body
        for stmt in body {
            self.write_indent();
            self.emit_node(stmt);
            self.write_line();
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
    }

    fn emit_comma_separated(&mut self, nodes: &[IRNode]) {
        for (i, node) in nodes.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.emit_node(node);
        }
    }

    fn emit_object_literal_multiline(&mut self, properties: &[IRProperty]) {
        if properties.is_empty() {
            self.write("{}");
            return;
        }
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
    }

    fn is_done_value_object_literal(properties: &[IRProperty]) -> bool {
        if properties.len() != 2 {
            return false;
        }
        let mut has_done = false;
        let mut has_value = false;
        for prop in properties {
            match (&prop.key, prop.kind) {
                (IRPropertyKey::Identifier(name), IRPropertyKind::Init) if name == "done" => {
                    has_done = true;
                }
                (IRPropertyKey::Identifier(name), IRPropertyKind::Init) if name == "value" => {
                    has_value = true;
                }
                _ => return false,
            }
        }
        has_done && has_value
    }

    fn emit_parameters(&mut self, params: &[IRParam]) {
        for (i, param) in params.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            if param.rest {
                self.write("...");
            }
            self.write(&param.name);
        }
    }

    fn emit_property(&mut self, prop: &IRProperty) {
        // Special case: spread property (key is "..." and value is SpreadElement)
        // Should emit as `...expr` not `"...": ...expr`
        if let IRPropertyKey::Identifier(name) = &prop.key {
            if name == "..." {
                if let IRNode::SpreadElement(inner) = &prop.value {
                    self.write("...");
                    self.emit_node(inner);
                    return;
                }
            }
        }

        match &prop.key {
            IRPropertyKey::Identifier(name) => self.write(name),
            IRPropertyKey::StringLiteral(s) => {
                self.write("\"");
                self.write_escaped(s);
                self.write("\"");
            }
            IRPropertyKey::NumericLiteral(n) => self.write(n),
            IRPropertyKey::Computed(expr) => {
                self.write("[");
                self.emit_node(expr);
                self.write("]");
            }
        }

        match prop.kind {
            IRPropertyKind::Init => {
                self.write(": ");
                self.emit_node(&prop.value);
            }
            IRPropertyKind::Get => {
                self.write(" ");
                self.emit_node(&prop.value);
            }
            IRPropertyKind::Set => {
                self.write(" ");
                self.emit_node(&prop.value);
            }
        }
    }

    fn emit_method_name(&mut self, name: &IRMethodName) {
        match name {
            IRMethodName::Identifier(n) => {
                self.write(".");
                self.write(n);
            }
            IRMethodName::StringLiteral(s) => {
                self.write("[\"");
                self.write_escaped(s);
                self.write("\"]");
            }
            IRMethodName::NumericLiteral(n) => {
                self.write("[");
                self.write(n);
                self.write("]");
            }
            IRMethodName::Computed(expr) => {
                self.write("[");
                self.emit_node(expr);
                self.write("]");
            }
        }
    }

    fn emit_switch_case(&mut self, case: &IRSwitchCase) {
        self.write_indent();
        if let Some(test) = &case.test {
            self.write("case ");
            self.emit_node(test);
            self.write(":");
        } else {
            self.write("default:");
        }
        self.write_line();

        self.increase_indent();
        for stmt in &case.statements {
            self.write_indent();
            self.emit_node(stmt);
            self.write_line();
        }
        self.decrease_indent();
    }

    fn write(&mut self, s: &str) {
        self.output.push_str(s);
    }

    fn write_escaped(&mut self, s: &str) {
        for c in s.chars() {
            match c {
                '"' => self.output.push_str("\\\""),
                '\\' => self.output.push_str("\\\\"),
                '\n' => self.output.push_str("\\n"),
                '\r' => self.output.push_str("\\r"),
                '\t' => self.output.push_str("\\t"),
                '\0' => self.output.push_str("\\0"),
                c if (c as u32) < 0x20 || c == '\x7F' => {
                    // Escape control characters as \u00NN (matching TypeScript format)
                    write!(self.output, "\\u{:04X}", c as u32).unwrap();
                }
                _ => self.output.push(c),
            }
        }
    }

    fn write_line(&mut self) {
        self.output.push('\n');
    }

    fn write_indent(&mut self) {
        for _ in 0..self.indent_level {
            self.output.push_str(self.indent_str);
        }
    }

    fn increase_indent(&mut self) {
        self.indent_level += 1;
    }

    fn decrease_indent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }

    /// Emit a multiline comment with proper indentation for each line.
    /// Normalizes indentation to match TypeScript's output format:
    /// - First line: current indentation + comment start (`/**`)
    /// - Subsequent lines: current indentation + ` *` or ` */`
    fn emit_multiline_comment(&mut self, comment: &str) {
        let mut first = true;
        for line in comment.split('\n') {
            if !first {
                self.write_line();
                self.write_indent();
            }
            // Strip leading whitespace, then add one space before * or */
            let trimmed = line.trim_start();
            if !first && (trimmed.starts_with('*') || trimmed.starts_with('/')) {
                self.write(" ");
            }
            self.write(trimmed.trim_end());
            first = false;
        }
    }
}

impl Default for IRPrinter<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> IRPrinter<'a> {
    /// Emit an arrow function as ES5 function expression using directive flags
    /// Transforms: () => expr  →  function () { return expr; }
    ///
    /// This is the NEW implementation that:
    /// 1. Uses flags from TransformDirective (doesn't re-calculate)
    /// 2. Uses recursive emit_node calls for the body (handles nested directives)
    /// 3. Supports class_alias for static class members
    fn emit_arrow_function_es5_with_flags(
        &mut self,
        arena: &NodeArena,
        _node: &Node,
        func: &tsz_parser::parser::node::FunctionData,
        _node_idx: NodeIndex,
        _captures_this: bool,
        _captures_arguments: bool,
        _class_alias: Option<String>,
    ) {
        use tsz_parser::parser::syntax_kind_ext;

        // Arrow functions are transformed to regular function expressions.
        // `this` capture is handled by `var _this = this;` at the enclosing
        // function scope. The lowering pass marks `this` references with
        // SubstituteThis to emit `_this` instead.

        self.write("function ");

        // Parameters
        self.write("(");
        let params = &func.parameters.nodes;
        for (i, &param_idx) in params.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            if let Some(param_node) = arena.get(param_idx) {
                if let Some(_param) = arena.get_parameter(param_node) {
                    if let Some(ident) = arena.get_identifier(param_node) {
                        self.write(&ident.escaped_text);
                    }
                }
            }
        }
        self.write(") ");

        // Body - use recursive emit_node to handle nested directives
        let body_node = arena.get(func.body);
        let is_block = body_node
            .map(|n| n.kind == syntax_kind_ext::BLOCK)
            .unwrap_or(false);

        if is_block {
            // Block body - emit recursively to handle nested transforms
            self.emit_node(&IRNode::ASTRef(func.body));
        } else {
            // Concise body - wrap with return and emit recursively
            // If body resolves to an object literal, wrap in parens
            let needs_parens = Self::concise_body_needs_parens(arena, func.body);
            if needs_parens {
                self.write("{ return (");
                self.emit_node(&IRNode::ASTRef(func.body));
                self.write("); }");
            } else {
                self.write("{ return ");
                self.emit_node(&IRNode::ASTRef(func.body));
                self.write("; }");
            }
        }
    }

    /// Check if a concise arrow body resolves to an object literal expression
    /// and needs wrapping in parens. Returns false if already parenthesized.
    fn concise_body_needs_parens(arena: &NodeArena, body_idx: NodeIndex) -> bool {
        let mut idx = body_idx;
        loop {
            let Some(node) = arena.get(idx) else {
                return false;
            };
            match node.kind {
                k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => return true,
                k if k == syntax_kind_ext::TYPE_ASSERTION
                    || k == syntax_kind_ext::AS_EXPRESSION =>
                {
                    if let Some(ta) = arena.get_type_assertion(node) {
                        idx = ta.expression;
                    } else {
                        return false;
                    }
                }
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => return false,
                _ => return false,
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/ir_printer.rs"]
mod tests;
