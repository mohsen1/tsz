//! ES5 Async Function Transform
//!
//! Transforms async functions to ES5 generators wrapped in __awaiter.
//!
//! # Transform Patterns
//!
//! ## Simple async function (no await)
//! ```typescript
//! async function foo(): Promise<void> { }
//! ```
//! Becomes:
//! ```javascript
//! function foo() {
//!     return __awaiter(this, void 0, void 0, function () {
//!         return __generator(this, function (_a) {
//!             return [2 /*return*/];
//!         });
//!     });
//! }
//! ```
//!
//! ## Async function with await
//! ```typescript
//! async function foo() {
//!     await bar();
//!     return 1;
//! }
//! ```
//! Becomes:
//! ```javascript
//! function foo() {
//!     return __awaiter(this, void 0, void 0, function () {
//!         return __generator(this, function (_a) {
//!             switch (_a.label) {
//!                 case 0: return [4 /*yield*/, bar()];
//!                 case 1:
//!                     _a.sent();
//!                     return [2 /*return*/, 1];
//!             }
//!         });
//!     });
//! }
//! ```
//!
//! ## Async arrow function
//! ```typescript
//! var foo = async () => { };
//! ```
//! Becomes:
//! ```javascript
//! var _this = this;
//! var foo = function () { return __awaiter(_this, void 0, void 0, function () {
//!     return __generator(this, function (_a) {
//!         return [2 /*return*/];
//!     });
//! }); };
//! ```

use crate::parser::thin_node::{ThinNode, ThinNodeArena};
use crate::parser::{NodeIndex, NodeList, syntax_kind_ext};
use crate::scanner::SyntaxKind;
use crate::source_map::Mapping;
use crate::source_writer::source_position_from_offset;
use crate::thin_emitter::ThinPrinter;
use crate::transform_context::{TransformContext, TransformDirective};
use crate::transforms::arrow_es5::contains_this_reference;
use crate::transforms::emit_utils;
use crate::transforms::private_fields_es5::{get_private_field_name, is_private_identifier};
use memchr;

/// Maximum recursion depth for emit_expression to prevent infinite loops
const MAX_RECURSION_DEPTH: u32 = 1000;

/// State for tracking async function transformation
#[derive(Debug, Default)]
pub struct AsyncTransformState {
    /// Current label counter for generator switch/case
    pub label_counter: u32,
    /// Whether we're currently inside an async function body
    pub in_async_body: bool,
    /// Whether any await expressions were found (determines if we need switch/case)
    pub has_await: bool,
}

impl AsyncTransformState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset for a new async function
    pub fn reset(&mut self) {
        self.label_counter = 0;
        self.in_async_body = false;
        self.has_await = false;
    }

    /// Get the next label number
    pub fn next_label(&mut self) -> u32 {
        let label = self.label_counter;
        self.label_counter += 1;
        label
    }
}

/// Async ES5 emitter for transforming async functions
pub struct AsyncES5Emitter<'a> {
    arena: &'a ThinNodeArena,
    output: String,
    indent_level: u32,
    source_text: Option<&'a str>,
    source_index: u32,
    mappings: Vec<Mapping>,
    line: u32,
    column: u32,
    state: AsyncTransformState,
    this_capture_depth: u32,
    /// Class name for private field access (e.g., "Foo" for _Foo_field)
    class_name: Option<String>,
    /// Recursion depth counter for emit_expression
    recursion_depth: u32,
}

impl<'a> AsyncES5Emitter<'a> {
    pub fn new(arena: &'a ThinNodeArena) -> Self {
        Self {
            arena,
            output: String::with_capacity(1024),
            indent_level: 0,
            source_text: None,
            source_index: 0,
            mappings: Vec::new(),
            line: 0,
            column: 0,
            state: AsyncTransformState::new(),
            this_capture_depth: 0,
            class_name: None,
            recursion_depth: 0,
        }
    }

    pub fn set_indent_level(&mut self, level: u32) {
        self.indent_level = level;
    }

    pub fn set_lexical_this(&mut self, capture: bool) {
        self.this_capture_depth = if capture { 1 } else { 0 };
    }

    pub fn set_use_this_capture(&mut self, capture: bool) {
        self.this_capture_depth = if capture { 1 } else { 0 };
    }

    /// Set the class name for private field access transformations
    pub fn set_class_name(&mut self, name: &str) {
        self.class_name = Some(name.to_string());
    }

    pub fn set_source_map_context(&mut self, source_text: &'a str, source_index: u32) {
        self.source_text = Some(source_text);
        self.source_index = source_index;
    }

    pub fn take_mappings(&mut self) -> Vec<Mapping> {
        std::mem::take(&mut self.mappings)
    }

    fn reset_output(&mut self) {
        self.output.clear();
        self.mappings.clear();
        self.line = 0;
        self.column = 0;
    }

    fn record_mapping(&mut self, node: &ThinNode) {
        let Some(text) = self.source_text else {
            return;
        };

        let source_pos = source_position_from_offset(text, node.pos);
        self.mappings.push(Mapping {
            generated_line: self.line,
            generated_column: self.column,
            source_index: self.source_index,
            original_line: source_pos.line,
            original_column: source_pos.column,
            name_index: None,
        });
    }

    /// Check if a function body contains any await expressions
    pub fn body_contains_await(&self, body_idx: NodeIndex) -> bool {
        self.contains_await_recursive(body_idx)
    }

    fn contains_await_recursive(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        // Check if this is an await expression
        if node.kind == syntax_kind_ext::AWAIT_EXPRESSION {
            return true;
        }

        // Don't recurse into nested functions (they have their own async context)
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || node.kind == syntax_kind_ext::ARROW_FUNCTION
        {
            return false;
        }

        // Check block statements
        if node.kind == syntax_kind_ext::BLOCK {
            if let Some(block) = self.arena.get_block(node) {
                for &stmt_idx in &block.statements.nodes {
                    if self.contains_await_recursive(stmt_idx) {
                        return true;
                    }
                }
            }
            return false;
        }

        // Check expression statements
        if node.kind == syntax_kind_ext::EXPRESSION_STATEMENT {
            if let Some(expr_stmt) = self.arena.get_expression_statement(node) {
                return self.contains_await_recursive(expr_stmt.expression);
            }
        }

        // Check return statements
        if node.kind == syntax_kind_ext::RETURN_STATEMENT {
            if let Some(ret) = self.arena.get_return_statement(node) {
                return self.contains_await_recursive(ret.expression);
            }
        }

        // Check variable statements
        if node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
            if let Some(var_data) = self.arena.get_variable(node) {
                for &decl_idx in &var_data.declarations.nodes {
                    if let Some(decl_node) = self.arena.get(decl_idx) {
                        if let Some(decl) = self.arena.get_variable_declaration(decl_node) {
                            if self.contains_await_recursive(decl.initializer) {
                                return true;
                            }
                        }
                    }
                }
            }
        }

        // Check if statements
        if node.kind == syntax_kind_ext::IF_STATEMENT {
            if let Some(if_stmt) = self.arena.get_if_statement(node) {
                if self.contains_await_recursive(if_stmt.expression) {
                    return true;
                }
                if self.contains_await_recursive(if_stmt.then_statement) {
                    return true;
                }
                if self.contains_await_recursive(if_stmt.else_statement) {
                    return true;
                }
            }
        }

        // Check call expressions
        if node.kind == syntax_kind_ext::CALL_EXPRESSION {
            if let Some(call) = self.arena.get_call_expr(node) {
                if self.contains_await_recursive(call.expression) {
                    return true;
                }
                if let Some(args) = &call.arguments {
                    for &arg_idx in &args.nodes {
                        if self.contains_await_recursive(arg_idx) {
                            return true;
                        }
                    }
                }
            }
        }

        // Check property/element access expressions
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            if let Some(access) = self.arena.get_access_expr(node) {
                if self.contains_await_recursive(access.expression) {
                    return true;
                }
                if self.contains_await_recursive(access.name_or_argument) {
                    return true;
                }
            }
        }

        // Check array/object literals (including computed property names and spreads)
        if node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            || node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
        {
            if let Some(literal) = self.arena.get_literal_expr(node) {
                for &elem_idx in &literal.elements.nodes {
                    let Some(elem_node) = self.arena.get(elem_idx) else {
                        continue;
                    };

                    match elem_node.kind {
                        syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                            if let Some(prop) = self.arena.get_property_assignment(elem_node) {
                                if self.computed_name_contains_await(prop.name) {
                                    return true;
                                }
                                if self.contains_await_recursive(prop.initializer) {
                                    return true;
                                }
                            }
                        }
                        syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                            if let Some(prop) = self.arena.get_shorthand_property(elem_node) {
                                if self.computed_name_contains_await(prop.name) {
                                    return true;
                                }
                                if self.contains_await_recursive(prop.object_assignment_initializer)
                                {
                                    return true;
                                }
                            }
                        }
                        syntax_kind_ext::SPREAD_ELEMENT => {
                            if let Some(spread) = self.arena.get_unary_expr_ex(elem_node) {
                                if self.contains_await_recursive(spread.expression) {
                                    return true;
                                }
                            }
                        }
                        syntax_kind_ext::METHOD_DECLARATION => {
                            if let Some(method) = self.arena.get_method_decl(elem_node) {
                                if self.computed_name_contains_await(method.name) {
                                    return true;
                                }
                            }
                        }
                        syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                            if let Some(accessor) = self.arena.get_accessor(elem_node) {
                                if self.computed_name_contains_await(accessor.name) {
                                    return true;
                                }
                            }
                        }
                        _ => {
                            if self.contains_await_recursive(elem_idx) {
                                return true;
                            }
                        }
                    }
                }
            }
            return false;
        }

        // Check conditional expressions
        if node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION {
            if let Some(cond) = self.arena.get_conditional_expr(node) {
                if self.contains_await_recursive(cond.condition) {
                    return true;
                }
                if self.contains_await_recursive(cond.when_true) {
                    return true;
                }
                if self.contains_await_recursive(cond.when_false) {
                    return true;
                }
            }
        }

        // Check binary expressions
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            if let Some(bin) = self.arena.get_binary_expr(node) {
                if self.contains_await_recursive(bin.left) {
                    return true;
                }
                if self.contains_await_recursive(bin.right) {
                    return true;
                }
            }
        }

        // Check prefix/postfix unary expressions
        if node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION
        {
            if let Some(unary) = self.arena.get_unary_expr(node) {
                return self.contains_await_recursive(unary.operand);
            }
        }

        // Check parenthesized expressions
        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            if let Some(paren) = self.arena.get_parenthesized(node) {
                return self.contains_await_recursive(paren.expression);
            }
        }

        // Check try/catch/finally statements
        if node.kind == syntax_kind_ext::TRY_STATEMENT {
            if let Some(try_data) = self.arena.get_try(node) {
                if self.contains_await_recursive(try_data.try_block) {
                    return true;
                }
                if self.contains_await_recursive(try_data.catch_clause) {
                    return true;
                }
                if self.contains_await_recursive(try_data.finally_block) {
                    return true;
                }
            }
        }

        // Check catch clauses
        if node.kind == syntax_kind_ext::CATCH_CLAUSE {
            if let Some(catch) = self.arena.get_catch_clause(node) {
                return self.contains_await_recursive(catch.block);
            }
        }

        // Check loop statements (while, do-while, for)
        if node.kind == syntax_kind_ext::WHILE_STATEMENT
            || node.kind == syntax_kind_ext::DO_STATEMENT
            || node.kind == syntax_kind_ext::FOR_STATEMENT
        {
            if let Some(loop_data) = self.arena.get_loop(node) {
                if self.contains_await_recursive(loop_data.initializer) {
                    return true;
                }
                if self.contains_await_recursive(loop_data.condition) {
                    return true;
                }
                if self.contains_await_recursive(loop_data.incrementor) {
                    return true;
                }
                if self.contains_await_recursive(loop_data.statement) {
                    return true;
                }
            }
        }

        // Check for-in/for-of statements
        if node.kind == syntax_kind_ext::FOR_IN_STATEMENT
            || node.kind == syntax_kind_ext::FOR_OF_STATEMENT
        {
            if let Some(for_data) = self.arena.get_for_in_of(node) {
                if self.contains_await_recursive(for_data.expression) {
                    return true;
                }
                if self.contains_await_recursive(for_data.statement) {
                    return true;
                }
            }
        }

        // Check switch statements
        if node.kind == syntax_kind_ext::SWITCH_STATEMENT {
            if let Some(switch_data) = self.arena.get_switch(node) {
                if self.contains_await_recursive(switch_data.expression) {
                    return true;
                }
                if self.contains_await_recursive(switch_data.case_block) {
                    return true;
                }
            }
        }

        // Check case blocks (uses block data with statements)
        if node.kind == syntax_kind_ext::CASE_BLOCK {
            if let Some(block_data) = self.arena.get_block(node) {
                for &stmt_idx in &block_data.statements.nodes {
                    if self.contains_await_recursive(stmt_idx) {
                        return true;
                    }
                }
            }
        }

        // Check case/default clauses
        if node.kind == syntax_kind_ext::CASE_CLAUSE || node.kind == syntax_kind_ext::DEFAULT_CLAUSE
        {
            if let Some(clause_data) = self.arena.get_case_clause(node) {
                if self.contains_await_recursive(clause_data.expression) {
                    return true;
                }
                for &stmt_idx in &clause_data.statements.nodes {
                    if self.contains_await_recursive(stmt_idx) {
                        return true;
                    }
                }
            }
        }

        // Check labeled statements
        if node.kind == syntax_kind_ext::LABELED_STATEMENT {
            if let Some(labeled_data) = self.arena.get_labeled_statement(node) {
                return self.contains_await_recursive(labeled_data.statement);
            }
        }

        // Check with statements (stored as IfStatementData)
        if node.kind == syntax_kind_ext::WITH_STATEMENT {
            if let Some(with_data) = self.arena.get_with_statement(node) {
                if self.contains_await_recursive(with_data.expression) {
                    return true;
                }
                if self.contains_await_recursive(with_data.then_statement) {
                    return true;
                }
            }
        }

        false
    }

    fn computed_name_contains_await(&self, name_idx: NodeIndex) -> bool {
        let Some(name_node) = self.arena.get(name_idx) else {
            return false;
        };

        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            if let Some(computed) = self.arena.get_computed_property(name_node) {
                return self.contains_await_recursive(computed.expression);
            }
        }

        false
    }

    /// Emit a simple async body with no await (inline format)
    /// Returns: "return __generator(this, function (_a) { return [2 /*return*/]; })"
    /// or with return value: "return __generator(this, function (_a) { return [2 /*return*/, expr]; })"
    pub fn emit_simple_generator_body(&mut self, body_idx: NodeIndex) -> String {
        self.reset_output();

        self.write("return __generator(this, function (_a) {");

        // Check if body is a block with a single return statement or empty
        let Some(body_node) = self.arena.get(body_idx) else {
            self.write(" return [2 /*return*/]; });");
            return std::mem::take(&mut self.output);
        };

        if body_node.kind == syntax_kind_ext::BLOCK {
            if let Some(block) = self.arena.get_block(body_node) {
                // Check for single return statement or empty block
                if block.statements.nodes.is_empty() {
                    self.write(" return [2 /*return*/]; });");
                    return std::mem::take(&mut self.output);
                }

                if block.statements.nodes.len() == 1 {
                    let stmt_idx = block.statements.nodes[0];
                    if let Some(stmt_node) = self.arena.get(stmt_idx) {
                        if stmt_node.kind == syntax_kind_ext::RETURN_STATEMENT {
                            if let Some(ret) = self.arena.get_return_statement(stmt_node) {
                                if ret.expression.is_none() {
                                    self.write(" return [2 /*return*/]; });");
                                } else {
                                    self.write(" return [2 /*return*/, ");
                                    self.emit_expression(ret.expression);
                                    self.write("]; });");
                                }
                                return std::mem::take(&mut self.output);
                            }
                        }
                    }
                }

                // For non-trivial blocks, emit statements inline.
                self.write_line();
                self.increase_indent();
                self.emit_async_body_statements(body_idx);
                self.decrease_indent();
                self.write_indent();
                self.write("});");
            }
        } else {
            // Concise arrow body - treat as return expression
            self.write(" return [2 /*return*/, ");
            self.emit_expression(body_idx);
            self.write("]; });");
        }

        std::mem::take(&mut self.output)
    }

    /// Emit a generator body with await (switch/case format)
    pub fn emit_generator_body_with_await(&mut self, body_idx: NodeIndex) -> String {
        self.reset_output();
        self.state.reset();
        self.state.has_await = true;

        self.write("return __generator(this, function (_a) {");
        self.write_line();
        self.increase_indent();

        // Start switch statement
        self.write_indent();
        self.write("switch (_a.label) {");
        self.write_line();
        self.increase_indent();

        // Emit case 0 (entry point)
        self.emit_case_label(0);

        // Process body statements
        self.emit_async_body_statements(body_idx);

        // Close switch
        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();

        // Close generator function
        self.decrease_indent();
        self.write_indent();
        self.write("});");

        std::mem::take(&mut self.output)
    }

    fn emit_async_body_statements(&mut self, body_idx: NodeIndex) {
        let Some(body_node) = self.arena.get(body_idx) else {
            // Empty body - just return
            self.write_indent();
            self.write("return [2 /*return*/];");
            self.write_line();
            return;
        };

        if body_node.kind == syntax_kind_ext::BLOCK {
            if let Some(block) = self.arena.get_block(body_node) {
                for &stmt_idx in &block.statements.nodes {
                    self.emit_async_statement(stmt_idx);
                }
            }
        } else {
            // Concise arrow - treat as return expression
            self.emit_return_with_possible_await(body_idx);
        }

        // Ensure we have a final return
        self.write_indent();
        self.write("return [2 /*return*/];");
        self.write_line();
    }

    fn emit_async_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };

        match stmt_node.kind {
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node) {
                    if self.is_await_expression(expr_stmt.expression) {
                        // await expr; -> return [4, expr]; case N: _a.sent();
                        self.emit_await_statement(expr_stmt.expression);
                    } else {
                        // Regular expression statement
                        self.write_indent();
                        self.emit_expression(expr_stmt.expression);
                        self.write(";");
                        self.write_line();
                    }
                }
            }
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret) = self.arena.get_return_statement(stmt_node) {
                    self.emit_return_with_possible_await(ret.expression);
                }
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                // Handle variable declarations with potential await
                self.emit_variable_statement_async(stmt_node);
            }
            _ => {
                // For other statements, just emit a placeholder for now
                // Full implementation would handle if/for/while/etc.
                self.write_indent();
                self.write("/* statement */;");
                self.write_line();
            }
        }
    }

    fn emit_await_statement(&mut self, await_idx: NodeIndex) {
        let Some(await_node) = self.arena.get(await_idx) else {
            return;
        };

        // Get the await operand (await uses UnaryExprDataEx, not UnaryExprData)
        let operand_idx = if await_node.has_data() {
            if let Some(unary_ex) = self
                .arena
                .unary_exprs_ex
                .get(await_node.data_index as usize)
            {
                unary_ex.expression
            } else {
                return;
            }
        } else {
            return;
        };

        // Emit: return [4 /*yield*/, operand];
        self.write_indent();
        self.write("return [4 /*yield*/, ");
        self.emit_expression(operand_idx);
        self.write("];");
        self.write_line();

        // Next case: _a.sent();
        self.state.label_counter += 1;
        self.emit_case_label(self.state.label_counter);
        self.write_indent();
        self.write("_a.sent();");
        self.write_line();
    }

    fn emit_return_with_possible_await(&mut self, expr_idx: NodeIndex) {
        if expr_idx.is_none() {
            self.write_indent();
            self.write("return [2 /*return*/];");
            self.write_line();
            return;
        }

        if self.is_await_expression(expr_idx) {
            // return await expr; -> return [4, expr]; case N: return [2, _a.sent()];
            let Some(await_node) = self.arena.get(expr_idx) else {
                return;
            };

            // await uses UnaryExprDataEx, not UnaryExprData
            let operand_idx = if await_node.has_data() {
                if let Some(unary_ex) = self
                    .arena
                    .unary_exprs_ex
                    .get(await_node.data_index as usize)
                {
                    unary_ex.expression
                } else {
                    return;
                }
            } else {
                return;
            };

            self.write_indent();
            self.write("return [4 /*yield*/, ");
            self.emit_expression(operand_idx);
            self.write("];");
            self.write_line();

            self.state.label_counter += 1;
            self.emit_case_label(self.state.label_counter);
            self.write_indent();
            self.write("return [2 /*return*/, _a.sent()];");
            self.write_line();
        } else {
            self.write_indent();
            self.write("return [2 /*return*/, ");
            self.emit_expression(expr_idx);
            self.write("];");
            self.write_line();
        }
    }

    fn emit_variable_statement_async(&mut self, stmt_node: &crate::parser::thin_node::ThinNode) {
        let Some(var_data) = self.arena.get_variable(stmt_node) else {
            return;
        };

        for &decl_list_idx in &var_data.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };

            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                continue;
            };

            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };

                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    continue;
                };

                if self.contains_await_recursive(decl.initializer) {
                    // Handle await in initializer
                    // var x = await foo(); -> return [4, foo()]; case N: x = _a.sent();
                    let name = self.get_binding_name(decl.name);

                    if self.is_await_expression(decl.initializer) {
                        let Some(await_node) = self.arena.get(decl.initializer) else {
                            continue;
                        };

                        // await uses UnaryExprDataEx, not UnaryExprData
                        let operand_idx = if await_node.has_data() {
                            if let Some(unary_ex) = self
                                .arena
                                .unary_exprs_ex
                                .get(await_node.data_index as usize)
                            {
                                unary_ex.expression
                            } else {
                                continue;
                            }
                        } else {
                            continue;
                        };

                        self.write_indent();
                        self.write("return [4 /*yield*/, ");
                        self.emit_expression(operand_idx);
                        self.write("];");
                        self.write_line();

                        self.state.label_counter += 1;
                        self.emit_case_label(self.state.label_counter);
                        self.write_indent();
                        self.write(&name);
                        self.write(" = _a.sent();");
                        self.write_line();
                    }
                } else {
                    // Regular variable declaration
                    self.write_indent();
                    self.emit_expression(decl.name);
                    if !decl.initializer.is_none() {
                        self.write(" = ");
                        self.emit_expression(decl.initializer);
                    }
                    self.write(";");
                    self.write_line();
                }
            }
        }
    }

    fn get_binding_name(&self, name_idx: NodeIndex) -> String {
        let Some(node) = self.arena.get(name_idx) else {
            return String::new();
        };

        if let Some(ident) = self.arena.get_identifier(node) {
            return ident.escaped_text.clone();
        }

        String::new()
    }

    fn is_await_expression(&self, idx: NodeIndex) -> bool {
        if let Some(node) = self.arena.get(idx) {
            return node.kind == syntax_kind_ext::AWAIT_EXPRESSION;
        }
        false
    }

    fn emit_case_label(&mut self, label: u32) {
        // Case labels are indented less than the case body
        self.decrease_indent();
        self.write_indent();
        self.write("case ");
        self.write_u32(label);
        self.write(":");
        if label > 0 {
            self.write_line();
        } else {
            self.write(" ");
        }
        self.increase_indent();
    }

    /// Emit __classPrivateFieldGet(receiver, _ClassName_field, "f")
    fn emit_private_field_get(&mut self, receiver_idx: NodeIndex, name_idx: NodeIndex) {
        let field_name = get_private_field_name(self.arena, name_idx).unwrap_or_default();
        let class_name = self.class_name.clone().unwrap_or_else(|| "_".to_string());

        self.write("__classPrivateFieldGet(");
        self.emit_expression(receiver_idx);
        self.write(", _");
        self.write(&class_name);
        self.write("_");
        self.write(&field_name);
        self.write(", \"f\")");
    }

    fn emit_expression(&mut self, idx: NodeIndex) {
        // Recursion depth check to prevent infinite loops
        self.recursion_depth += 1;
        if self.recursion_depth > MAX_RECURSION_DEPTH {
            self.write("/* recursion limit exceeded */");
            self.recursion_depth -= 1;
            return;
        }

        let Some(node) = self.arena.get(idx) else {
            self.recursion_depth -= 1;
            return;
        };

        match node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    self.record_mapping(node);
                    self.write(&lit.text);
                }
            }
            k if k == SyntaxKind::StringLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    self.record_mapping(node);
                    self.write("\"");
                    self.write(&lit.text);
                    self.write("\"");
                }
            }
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(ident) = self.arena.get_identifier(node) {
                    self.record_mapping(node);
                    self.write(&ident.escaped_text);
                }
            }
            k if k == SyntaxKind::TrueKeyword as u16 => {
                self.record_mapping(node);
                self.write("true");
            }
            k if k == SyntaxKind::FalseKeyword as u16 => {
                self.record_mapping(node);
                self.write("false");
            }
            k if k == SyntaxKind::NullKeyword as u16 => {
                self.record_mapping(node);
                self.write("null");
            }
            k if k == SyntaxKind::UndefinedKeyword as u16 => {
                self.record_mapping(node);
                self.write("undefined");
            }
            k if k == SyntaxKind::ThisKeyword as u16 => {
                self.record_mapping(node);
                if self.this_capture_depth > 0 {
                    self.write("_this");
                } else {
                    self.write("this");
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(node) {
                    if self.is_super_method_call(call.expression) {
                        self.emit_super_method_call(call.expression, &call.arguments);
                    } else if self.is_super_element_call(call.expression) {
                        self.emit_super_element_call(call.expression, &call.arguments);
                    } else {
                        self.emit_expression(call.expression);
                        self.write("(");
                        if let Some(args) = &call.arguments {
                            let mut first = true;
                            for &arg_idx in &args.nodes {
                                if !first {
                                    self.write(", ");
                                }
                                first = false;
                                self.emit_expression(arg_idx);
                            }
                        }
                        self.write(")");
                    }
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node) {
                    // Check if this is a private field access (this.#field)
                    if is_private_identifier(self.arena, access.name_or_argument) {
                        self.emit_private_field_get(access.expression, access.name_or_argument);
                    } else {
                        self.emit_expression(access.expression);
                        self.write(".");
                        self.emit_expression(access.name_or_argument);
                    }
                }
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node) {
                    self.emit_expression(access.expression);
                    self.write("[");
                    self.emit_expression(access.name_or_argument);
                    self.write("]");
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.arena.get_binary_expr(node) {
                    self.emit_expression(bin.left);
                    self.write(" ");
                    self.emit_operator(bin.operator_token);
                    self.write(" ");
                    self.emit_expression(bin.right);
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.arena.get_unary_expr(node) {
                    self.emit_operator(unary.operator);
                    self.emit_expression(unary.operand);
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.arena.get_parenthesized(node) {
                    self.write("(");
                    self.emit_expression(paren.expression);
                    self.write(")");
                }
            }
            k if k == syntax_kind_ext::AWAIT_EXPRESSION => {
                // For expressions like return await x, we emit just the operand
                // (the await is handled by the state machine)
                if let Some(unary) = self.arena.get_unary_expr_ex(node) {
                    if !unary.expression.is_none() {
                        self.emit_expression(unary.expression);
                    }
                }
            }
            k if k == syntax_kind_ext::ARROW_FUNCTION => {
                if self.contains_super_reference(idx) {
                    self.emit_arrow_function_with_super(idx);
                } else {
                    let captures_this = contains_this_reference(self.arena, idx);
                    let mut transforms = TransformContext::new();
                    transforms.insert(
                        idx,
                        TransformDirective::ES5ArrowFunction {
                            arrow_node: idx,
                            captures_this,
                        },
                    );
                    let mut printer = ThinPrinter::with_transforms(self.arena, transforms);
                    printer.set_target_es5(true);
                    printer.emit(idx);
                    self.write(printer.get_output());
                }
            }
            _ => {
                // Fallback for unhandled expressions
                self.write("void 0");
            }
        }

        self.recursion_depth -= 1;
    }

    fn is_super_method_call(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };

        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }

        let Some(access) = self.arena.get_access_expr(expr_node) else {
            return false;
        };
        let Some(base_node) = self.arena.get(access.expression) else {
            return false;
        };
        base_node.kind == SyntaxKind::SuperKeyword as u16
    }

    fn is_super_element_call(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };

        if expr_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            return false;
        }

        let Some(access) = self.arena.get_access_expr(expr_node) else {
            return false;
        };
        let Some(base_node) = self.arena.get(access.expression) else {
            return false;
        };

        base_node.kind == SyntaxKind::SuperKeyword as u16
    }

    fn contains_super_reference(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        if node.kind == SyntaxKind::SuperKeyword as u16 {
            return true;
        }

        match node.kind {
            k if k == syntax_kind_ext::BLOCK => {
                if let Some(block) = self.arena.get_block(node) {
                    for &stmt_idx in &block.statements.nodes {
                        if self.contains_super_reference(stmt_idx) {
                            return true;
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr_stmt) = self.arena.get_expression_statement(node) {
                    return self.contains_super_reference(expr_stmt.expression);
                }
            }
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret) = self.arena.get_return_statement(node) {
                    if !ret.expression.is_none() {
                        return self.contains_super_reference(ret.expression);
                    }
                }
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_data) = self.arena.get_variable(node) {
                    for &decl_idx in &var_data.declarations.nodes {
                        if let Some(decl_node) = self.arena.get(decl_idx) {
                            if let Some(decl) = self.arena.get_variable_declaration(decl_node) {
                                if !decl.initializer.is_none()
                                    && self.contains_super_reference(decl.initializer)
                                {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(node) {
                    if self.contains_super_reference(call.expression) {
                        return true;
                    }
                    if let Some(args) = &call.arguments {
                        for &arg_idx in &args.nodes {
                            if self.contains_super_reference(arg_idx) {
                                return true;
                            }
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                if let Some(access) = self.arena.get_access_expr(node) {
                    if self.contains_super_reference(access.expression) {
                        return true;
                    }
                    if self.contains_super_reference(access.name_or_argument) {
                        return true;
                    }
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = self.arena.get_conditional_expr(node) {
                    if self.contains_super_reference(cond.condition)
                        || self.contains_super_reference(cond.when_true)
                        || self.contains_super_reference(cond.when_false)
                    {
                        return true;
                    }
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.arena.get_binary_expr(node) {
                    if self.contains_super_reference(bin.left)
                        || self.contains_super_reference(bin.right)
                    {
                        return true;
                    }
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                || k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION =>
            {
                if let Some(unary) = self.arena.get_unary_expr(node) {
                    return self.contains_super_reference(unary.operand);
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.arena.get_parenthesized(node) {
                    return self.contains_super_reference(paren.expression);
                }
            }
            k if k == syntax_kind_ext::ARROW_FUNCTION => {
                if let Some(func) = self.arena.get_function(node) {
                    if !func.body.is_none() && self.contains_super_reference(func.body) {
                        return true;
                    }
                }
            }
            _ => {}
        }

        false
    }

    fn emit_super_method_call(&mut self, callee_idx: NodeIndex, args: &Option<NodeList>) {
        let Some(callee_node) = self.arena.get(callee_idx) else {
            return;
        };
        let Some(access) = self.arena.get_access_expr(callee_node) else {
            return;
        };

        self.write("_super.prototype.");
        self.emit_expression(access.name_or_argument);
        self.write(".call(");
        if self.this_capture_depth > 0 {
            self.write("_this");
        } else {
            self.write("this");
        }

        if let Some(arg_list) = args {
            for &arg_idx in &arg_list.nodes {
                self.write(", ");
                self.emit_expression(arg_idx);
            }
        }
        self.write(")");
    }

    fn emit_super_element_call(&mut self, callee_idx: NodeIndex, args: &Option<NodeList>) {
        let Some(callee_node) = self.arena.get(callee_idx) else {
            return;
        };
        let Some(access) = self.arena.get_access_expr(callee_node) else {
            return;
        };

        self.write("_super.prototype[");
        self.emit_expression(access.name_or_argument);
        self.write("].call(");
        if self.this_capture_depth > 0 {
            self.write("_this");
        } else {
            self.write("this");
        }

        if let Some(arg_list) = args {
            for &arg_idx in &arg_list.nodes {
                self.write(", ");
                self.emit_expression(arg_idx);
            }
        }

        self.write(")");
    }

    fn emit_arrow_function_with_super(&mut self, arrow_idx: NodeIndex) {
        let Some(arrow_node) = self.arena.get(arrow_idx) else {
            return;
        };
        let Some(func) = self.arena.get_function(arrow_node) else {
            return;
        };

        let captures_this = contains_this_reference(self.arena, arrow_idx);
        let parent_this_expr = if self.this_capture_depth > 0 {
            "_this"
        } else {
            "this"
        };

        if captures_this {
            self.write("(function (_this) { return ");
            self.this_capture_depth += 1;
        }

        self.write("function (");
        self.emit_arrow_parameters_simple(&func.parameters);
        self.write(") ");

        let body_node = self.arena.get(func.body);
        let is_block = body_node
            .map(|node| node.kind == syntax_kind_ext::BLOCK)
            .unwrap_or(false);

        if is_block {
            self.emit_arrow_block(func.body);
        } else {
            self.write("{ return ");
            self.emit_expression(func.body);
            self.write("; }");
        }

        if captures_this {
            self.this_capture_depth -= 1;
            self.write("; })(");
            self.write(parent_this_expr);
            self.write(")");
        }
    }

    fn emit_arrow_parameters_simple(&mut self, params: &NodeList) {
        let mut first = true;
        for &param_idx in &params.nodes {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            if !first {
                self.write(", ");
            }
            first = false;
            if param.dot_dot_dot_token {
                self.write("...");
            }
            if !param.name.is_none() {
                self.emit_expression(param.name);
            }
        }
    }

    fn emit_arrow_block(&mut self, block_idx: NodeIndex) {
        let Some(block_node) = self.arena.get(block_idx) else {
            self.write("{ }");
            return;
        };
        let Some(block) = self.arena.get_block(block_node) else {
            self.write("{ }");
            return;
        };

        self.write("{");
        self.write_line();
        self.increase_indent();
        for &stmt_idx in &block.statements.nodes {
            self.emit_arrow_statement(stmt_idx);
        }
        self.decrease_indent();
        self.write_indent();
        self.write("}");
    }

    fn emit_arrow_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };

        match stmt_node.kind {
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node) {
                    self.write_indent();
                    self.emit_expression(expr_stmt.expression);
                    self.write(";");
                    self.write_line();
                }
            }
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret) = self.arena.get_return_statement(stmt_node) {
                    self.write_indent();
                    self.write("return");
                    if !ret.expression.is_none() {
                        self.write(" ");
                        self.emit_expression(ret.expression);
                    }
                    self.write(";");
                    self.write_line();
                }
            }
            _ => {
                self.write_indent();
                self.write("/* statement */;");
                self.write_line();
            }
        }
    }

    fn emit_operator(&mut self, op: u16) {
        let op_str = match op {
            k if k == SyntaxKind::PlusToken as u16 => "+",
            k if k == SyntaxKind::MinusToken as u16 => "-",
            k if k == SyntaxKind::AsteriskToken as u16 => "*",
            k if k == SyntaxKind::SlashToken as u16 => "/",
            k if k == SyntaxKind::PercentToken as u16 => "%",
            k if k == SyntaxKind::PlusPlusToken as u16 => "++",
            k if k == SyntaxKind::MinusMinusToken as u16 => "--",
            k if k == SyntaxKind::EqualsToken as u16 => "=",
            k if k == SyntaxKind::PlusEqualsToken as u16 => "+=",
            k if k == SyntaxKind::MinusEqualsToken as u16 => "-=",
            k if k == SyntaxKind::EqualsEqualsToken as u16 => "==",
            k if k == SyntaxKind::EqualsEqualsEqualsToken as u16 => "===",
            k if k == SyntaxKind::ExclamationEqualsToken as u16 => "!=",
            k if k == SyntaxKind::ExclamationEqualsEqualsToken as u16 => "!==",
            k if k == SyntaxKind::LessThanToken as u16 => "<",
            k if k == SyntaxKind::GreaterThanToken as u16 => ">",
            k if k == SyntaxKind::LessThanEqualsToken as u16 => "<=",
            k if k == SyntaxKind::GreaterThanEqualsToken as u16 => ">=",
            k if k == SyntaxKind::AmpersandAmpersandToken as u16 => "&&",
            k if k == SyntaxKind::BarBarToken as u16 => "||",
            k if k == SyntaxKind::ExclamationToken as u16 => "!",
            k if k == SyntaxKind::TildeToken as u16 => "~",
            k if k == SyntaxKind::AmpersandToken as u16 => "&",
            k if k == SyntaxKind::BarToken as u16 => "|",
            k if k == SyntaxKind::CaretToken as u16 => "^",
            _ => "/* op */",
        };
        self.write(op_str);
    }

    fn write(&mut self, s: &str) {
        self.output.push_str(s);
        self.advance_position(s);
    }

    fn write_u32(&mut self, value: u32) {
        emit_utils::push_u32(&mut self.output, value);
        let mut remaining = value;
        let mut digits = 1;
        while remaining >= 10 {
            remaining /= 10;
            digits += 1;
        }
        self.column += digits;
    }

    fn write_line(&mut self) {
        self.output.push('\n');
        self.line += 1;
        self.column = 0;
    }

    fn write_indent(&mut self) {
        for _ in 0..self.indent_level {
            self.output.push_str("    ");
        }
        self.column += self.indent_level * 4;
    }

    fn increase_indent(&mut self) {
        self.indent_level += 1;
    }

    fn decrease_indent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }

    fn advance_position(&mut self, text: &str) {
        let bytes = text.as_bytes();
        let mut i = 0;

        while i < bytes.len() {
            match memchr::memchr(b'\n', &bytes[i..]) {
                Some(offset) => {
                    let segment_end = i + offset;
                    let segment = &text[i..segment_end];

                    if segment.is_ascii() {
                        self.column += segment.len() as u32;
                    } else {
                        self.column += segment.chars().map(|c| c.len_utf16() as u32).sum::<u32>();
                    }

                    self.line += 1;
                    self.column = 0;
                    i = segment_end + 1;
                }
                None => {
                    let segment = &text[i..];
                    if segment.is_ascii() {
                        self.column += segment.len() as u32;
                    } else {
                        self.column += segment.chars().map(|c| c.len_utf16() as u32).sum::<u32>();
                    }
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "async_es5_tests.rs"]
mod async_es5_tests;
