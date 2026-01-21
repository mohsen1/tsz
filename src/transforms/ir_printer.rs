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

use crate::parser::node::NodeArena;
use crate::transforms::ir::*;

/// IR Printer - converts IR nodes to JavaScript strings
pub struct IRPrinter<'a> {
    output: String,
    indent_level: u32,
    indent_str: &'static str,
    /// Optional arena for handling ASTRef nodes
    arena: Option<&'a NodeArena>,
    /// Source text for emitting ASTRef nodes
    source_text: Option<&'a str>,
}

impl<'a> IRPrinter<'a> {
    /// Create a new IR printer
    pub fn new() -> Self {
        Self {
            output: String::with_capacity(4096),
            indent_level: 0,
            indent_str: "    ",
            arena: None,
            source_text: None,
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
        }
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
        self.emit_node(node);
        &self.output
    }

    /// Emit an IR node and return the output
    pub fn emit_to_string(node: &IRNode) -> String {
        let mut printer = Self::new();
        printer.emit_node(node);
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
                self.write(" ");
                self.write(operator);
                self.write(" ");
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
                self.emit_node(callee);
                self.write("(");
                self.emit_comma_separated(arguments);
                self.write(")");
            }
            IRNode::NewExpr { callee, arguments } => {
                self.write("new ");
                self.emit_node(callee);
                self.write("(");
                self.emit_comma_separated(arguments);
                self.write(")");
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
            IRNode::ObjectLiteral(props) => {
                if props.is_empty() {
                    self.write("{}");
                } else {
                    self.write("{ ");
                    for (i, prop) in props.iter().enumerate() {
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
            } => {
                self.write("function ");
                if let Some(n) = name {
                    self.write(n);
                }
                self.write("(");
                self.emit_parameters(parameters);
                self.write(") ");
                if *is_expression_body && body.len() == 1 {
                    if let IRNode::ReturnStatement(Some(expr)) = &body[0] {
                        self.write("{ return ");
                        self.emit_node(expr);
                        self.write("; }");
                        return;
                    }
                }
                self.emit_block(body);
            }
            IRNode::LogicalOr { left, right } => {
                self.emit_node(left);
                self.write(" || ");
                self.emit_node(right);
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
            }
            IRNode::ExpressionStatement(expr) => {
                self.emit_node(expr);
                self.write(";");
            }
            IRNode::ReturnStatement(expr) => {
                self.write("return");
                if let Some(e) = expr {
                    self.write(" ");
                    self.emit_node(e);
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
                    self.write(" else ");
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
            } => {
                self.write("function ");
                self.write(name);
                self.write("(");
                self.emit_parameters(parameters);
                self.write(") ");
                self.emit_block(body);
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
                    self.write_indent();
                    self.emit_node(stmt);
                    self.write_line();
                }

                self.decrease_indent();
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
            } => {
                self.write(class_name);
                self.write(".prototype");
                self.emit_method_name(method_name);
                self.write(" = ");
                self.emit_node(function);
                self.write(";");
            }
            IRNode::StaticMethod {
                class_name,
                method_name,
                function,
            } => {
                self.write(class_name);
                self.emit_method_name(method_name);
                self.write(" = ");
                self.emit_node(function);
                self.write(";");
            }
            IRNode::DefineProperty {
                target,
                property_name,
                descriptor,
            } => {
                self.write("Object.defineProperty(");
                self.emit_node(target);
                self.write(", \"");
                self.write(property_name);
                self.write("\", {");
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
            IRNode::Sequence(nodes) => {
                for node in nodes {
                    self.emit_node(node);
                }
            }
            IRNode::ASTRef(idx) => {
                // Fallback: emit a placeholder or use the arena if available
                if let Some(arena) = self.arena {
                    if let Some(text) = self.source_text {
                        if let Some(node) = arena.get(*idx) {
                            let start = node.pos as usize;
                            let end = node.end as usize;
                            if start < end && end <= text.len() {
                                self.write(&text[start..end]);
                                return;
                            }
                        }
                    }
                }
                self.write("/* ASTRef */");
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
                self.write("(function (");
                self.write(name);
                self.write(") {");
                self.write_line();
                self.increase_indent();

                // Emit members
                for member in members {
                    self.emit_enum_member(name, member);
                    self.write_line();
                }

                self.decrease_indent();
                self.write("}(");
                self.write(name);
                self.write(" || (");
                self.write(name);
                self.write(" = {})));");
            }

            // Namespace Transform Specific
            IRNode::NamespaceIIFE {
                name: _name,
                name_parts,
                body,
                is_exported,
                attach_to_exports,
            } => {
                self.emit_namespace_iife(&name_parts, 0, body, *is_exported, *attach_to_exports);
            }
            IRNode::NamespaceExport { namespace, name, value } => {
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
    ) {
        let current_name = &name_parts[index];
        let is_last = index == name_parts.len() - 1;

        // var name;
        self.write("var ");
        self.write(current_name);
        self.write(";");
        self.write_line();

        // Open IIFE
        self.write("(function (");
        self.write(current_name);
        self.write(") {");
        self.write_line();
        self.increase_indent();

        if is_last {
            // Emit body
            for stmt in body {
                self.write_indent();
                self.emit_node(stmt);
                self.write_line();
            }
        } else {
            // var next_name;
            let next_name = &name_parts[index + 1];
            self.write_indent();
            self.write("var ");
            self.write(next_name);
            self.write(";");
            self.write_line();
            // Recurse
            self.emit_namespace_iife(name_parts, index + 1, body, is_exported, attach_to_exports);
        }

        self.decrease_indent();
        self.write("}(");

        // Argument
        if index == 0 {
            self.write(current_name);
            if is_exported && attach_to_exports {
                self.write(" = exports.");
                self.write(current_name);
                self.write(" || (exports.");
                self.write(current_name);
                self.write(" = {})");
            } else {
                self.write(" || (");
                self.write(current_name);
                self.write(" = {})");
            }
        } else {
            let parent_name = &name_parts[index - 1];
            self.write(current_name);
            self.write(" = ");
            self.write(parent_name);
            self.write(".");
            self.write(current_name);
            self.write(" || (");
            self.write(parent_name);
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

    fn emit_comma_separated(&mut self, nodes: &[IRNode]) {
        for (i, node) in nodes.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.emit_node(node);
        }
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
}

impl Default for IRPrinter<'_> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emit_literals() {
        assert_eq!(IRPrinter::emit_to_string(&IRNode::number("42")), "42");
        assert_eq!(
            IRPrinter::emit_to_string(&IRNode::string("hello")),
            "\"hello\""
        );
        assert_eq!(
            IRPrinter::emit_to_string(&IRNode::BooleanLiteral(true)),
            "true"
        );
        assert_eq!(
            IRPrinter::emit_to_string(&IRNode::BooleanLiteral(false)),
            "false"
        );
        assert_eq!(IRPrinter::emit_to_string(&IRNode::NullLiteral), "null");
        assert_eq!(IRPrinter::emit_to_string(&IRNode::Undefined), "void 0");
    }

    #[test]
    fn test_emit_identifiers() {
        assert_eq!(IRPrinter::emit_to_string(&IRNode::id("foo")), "foo");
        assert_eq!(IRPrinter::emit_to_string(&IRNode::this()), "this");
        assert_eq!(IRPrinter::emit_to_string(&IRNode::this_captured()), "_this");
    }

    #[test]
    fn test_emit_binary_expr() {
        let expr = IRNode::binary(IRNode::id("a"), "+", IRNode::number("1"));
        assert_eq!(IRPrinter::emit_to_string(&expr), "a + 1");

        let assign = IRNode::assign(IRNode::id("x"), IRNode::number("42"));
        assert_eq!(IRPrinter::emit_to_string(&assign), "x = 42");
    }

    #[test]
    fn test_emit_call_expr() {
        let call = IRNode::call(IRNode::id("foo"), vec![]);
        assert_eq!(IRPrinter::emit_to_string(&call), "foo()");

        let call_args = IRNode::call(
            IRNode::id("bar"),
            vec![IRNode::number("1"), IRNode::string("test")],
        );
        assert_eq!(IRPrinter::emit_to_string(&call_args), "bar(1, \"test\")");
    }

    #[test]
    fn test_emit_property_access() {
        let prop = IRNode::prop(IRNode::id("obj"), "prop");
        assert_eq!(IRPrinter::emit_to_string(&prop), "obj.prop");

        let chained = IRNode::prop(IRNode::prop(IRNode::id("a"), "b"), "c");
        assert_eq!(IRPrinter::emit_to_string(&chained), "a.b.c");
    }

    #[test]
    fn test_emit_element_access() {
        let elem = IRNode::elem(IRNode::id("arr"), IRNode::number("0"));
        assert_eq!(IRPrinter::emit_to_string(&elem), "arr[0]");
    }

    #[test]
    fn test_emit_var_decl() {
        let decl = IRNode::var_decl("x", None);
        assert_eq!(IRPrinter::emit_to_string(&decl), "var x");

        let decl_init = IRNode::var_decl("y", Some(IRNode::number("42")));
        assert_eq!(IRPrinter::emit_to_string(&decl_init), "var y = 42");
    }

    #[test]
    fn test_emit_return_statement() {
        let ret = IRNode::ret(None);
        assert_eq!(IRPrinter::emit_to_string(&ret), "return;");

        let ret_val = IRNode::ret(Some(IRNode::number("42")));
        assert_eq!(IRPrinter::emit_to_string(&ret_val), "return 42;");
    }

    #[test]
    fn test_emit_function_decl() {
        let func = IRNode::func_decl(
            "foo",
            vec![IRParam::new("x")],
            vec![IRNode::ret(Some(IRNode::id("x")))],
        );
        let output = IRPrinter::emit_to_string(&func);
        assert!(output.contains("function foo(x)"));
        assert!(output.contains("return x;"));
    }

    #[test]
    fn test_emit_function_expr() {
        let func = IRNode::func_expr(None, vec![], vec![IRNode::ret(Some(IRNode::number("42")))]);
        let output = IRPrinter::emit_to_string(&func);
        assert!(output.contains("function ()"));
        assert!(output.contains("return 42;"));
    }

    #[test]
    fn test_emit_es5_class_iife() {
        let class = IRNode::ES5ClassIIFE {
            name: "Point".to_string(),
            base_class: None,
            body: vec![
                IRNode::func_decl(
                    "Point",
                    vec![IRParam::new("x")],
                    vec![IRNode::expr_stmt(IRNode::assign(
                        IRNode::prop(IRNode::this(), "x"),
                        IRNode::id("x"),
                    ))],
                ),
                IRNode::ret(Some(IRNode::id("Point"))),
            ],
            weakmap_decls: vec![],
            weakmap_inits: vec![],
        };

        let output = IRPrinter::emit_to_string(&class);
        assert!(output.contains("var Point = /** @class */ (function ()"));
        assert!(output.contains("function Point(x)"));
        assert!(output.contains("this.x = x"));
        assert!(output.contains("return Point;"));
    }

    #[test]
    fn test_emit_es5_class_with_extends() {
        let class = IRNode::ES5ClassIIFE {
            name: "Child".to_string(),
            base_class: Some(Box::new(IRNode::id("Parent"))),
            body: vec![
                IRNode::ExtendsHelper {
                    class_name: "Child".to_string(),
                },
                IRNode::func_decl("Child", vec![], vec![]),
                IRNode::ret(Some(IRNode::id("Child"))),
            ],
            weakmap_decls: vec![],
            weakmap_inits: vec![],
        };

        let output = IRPrinter::emit_to_string(&class);
        assert!(output.contains("(function (_super)"));
        assert!(output.contains("__extends(Child, _super)"));
        assert!(output.contains("}(Parent))"));
    }

    #[test]
    fn test_emit_generator_body_simple() {
        let generator_body = IRNode::GeneratorBody {
            has_await: false,
            cases: vec![IRGeneratorCase {
                label: 0,
                statements: vec![IRNode::ret(Some(IRNode::GeneratorOp {
                    opcode: 2,
                    value: None,
                    comment: Some("return".to_string()),
                }))],
            }],
        };

        let output = IRPrinter::emit_to_string(&generator_body);
        assert!(output.contains("return __generator(this, function (_a)"));
        assert!(output.contains("[2 /*return*/]"));
    }

    #[test]
    fn test_emit_awaiter_call() {
        let awaiter = IRNode::AwaiterCall {
            this_arg: Box::new(IRNode::this()),
            generator_body: Box::new(IRNode::GeneratorBody {
                has_await: false,
                cases: vec![IRGeneratorCase {
                    label: 0,
                    statements: vec![IRNode::ret(Some(IRNode::GeneratorOp {
                        opcode: 2,
                        value: None,
                        comment: Some("return".to_string()),
                    }))],
                }],
            }),
        };

        let output = IRPrinter::emit_to_string(&awaiter);
        assert!(output.contains("return __awaiter(this, void 0, void 0, function ()"));
    }

    #[test]
    fn test_emit_private_field_get() {
        let get = IRNode::PrivateFieldGet {
            receiver: Box::new(IRNode::this()),
            weakmap_name: "_Foo_bar".to_string(),
        };

        let output = IRPrinter::emit_to_string(&get);
        assert_eq!(output, "__classPrivateFieldGet(this, _Foo_bar, \"f\")");
    }

    #[test]
    fn test_emit_private_field_set() {
        let set = IRNode::PrivateFieldSet {
            receiver: Box::new(IRNode::this()),
            weakmap_name: "_Foo_bar".to_string(),
            value: Box::new(IRNode::number("42")),
        };

        let output = IRPrinter::emit_to_string(&set);
        assert_eq!(output, "__classPrivateFieldSet(this, _Foo_bar, 42, \"f\")");
    }

    #[test]
    fn test_emit_string_escaping() {
        let str_lit = IRNode::string("hello\nworld");
        assert_eq!(IRPrinter::emit_to_string(&str_lit), "\"hello\\nworld\"");

        let str_quotes = IRNode::string("say \"hi\"");
        assert_eq!(IRPrinter::emit_to_string(&str_quotes), "\"say \\\"hi\\\"\"");
    }
}
