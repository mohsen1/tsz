//! ES5 Async Function Transform (IR-based)
//!
//! Transforms async functions to ES5 generators wrapped in __awaiter.
//! This module produces IR nodes that are then printed by IRPrinter.
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
//! ## Architecture
//!
//! This transformer produces IR nodes using the established IR pattern.
//! The thin wrapper in `async_es5.rs` uses this transformer with `IRPrinter`
//! to emit JavaScript strings.

use crate::parser::NodeIndex;
use crate::parser::node::NodeArena;
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;
use crate::transforms::helpers::HelpersNeeded;
use crate::transforms::ir::{
    IRGeneratorCase, IRNode, IRParam, IRProperty, IRPropertyKey, IRPropertyKind,
};

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

/// Generator opcodes for the __generator helper
pub mod opcodes {
    /// Resume execution
    pub const NEXT: u32 = 0;
    /// Throw an error
    pub const THROW: u32 = 1;
    /// Return (complete)
    pub const RETURN: u32 = 2;
    /// Break to label
    pub const BREAK: u32 = 3;
    /// Yield a value (used for await)
    pub const YIELD: u32 = 4;
    /// Yield* delegation
    pub const YIELD_STAR: u32 = 5;
    /// Catch
    pub const CATCH: u32 = 6;
    /// End finally
    pub const END_FINALLY: u32 = 7;
}

/// Async ES5 transformer that produces IR nodes instead of strings.
///
/// This transformer mirrors the GeneratorES5Transformer pattern from generators.rs.
/// It converts async functions to ES5 code using __awaiter and __generator helpers.
pub struct AsyncES5Transformer<'a> {
    arena: &'a NodeArena,
    state: AsyncTransformState,
    helpers_needed: HelpersNeeded,
}

impl<'a> AsyncES5Transformer<'a> {
    /// Create a new AsyncES5Transformer
    pub fn new(arena: &'a NodeArena) -> Self {
        Self {
            arena,
            state: AsyncTransformState::new(),
            helpers_needed: HelpersNeeded::default(),
        }
    }

    /// Get the helpers needed after transformation
    pub fn get_helpers_needed(&self) -> &HelpersNeeded {
        &self.helpers_needed
    }

    /// Take the helpers needed (consumes the transformer)
    pub fn take_helpers_needed(self) -> HelpersNeeded {
        self.helpers_needed
    }

    /// Transform an async function declaration to IR
    ///
    /// Returns an IRNode::AwaiterCall with a nested IRNode::GeneratorBody
    pub fn transform_async_function(&mut self, func_idx: NodeIndex) -> IRNode {
        self.state.reset();
        self.helpers_needed.awaiter = true;
        self.helpers_needed.generator = true;

        let Some(node) = self.arena.get(func_idx) else {
            return IRNode::Undefined;
        };

        // Get function details - all function types use FunctionData
        let (name, params, body_idx) = if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || node.kind == syntax_kind_ext::ARROW_FUNCTION
        {
            if let Some(func) = self.arena.get_function(node) {
                let name = if func.name.is_none() {
                    None
                } else {
                    Some(self.get_identifier_text(func.name))
                };
                let params = self.collect_parameters(&func.parameters);
                (name, params, func.body)
            } else {
                return IRNode::Undefined;
            }
        } else {
            return IRNode::Undefined;
        };

        // Check if body contains await
        let has_await = self.body_contains_await(body_idx);
        self.state.has_await = has_await;

        // Build the generator body
        let generator_body = self.build_generator_body(body_idx, has_await);

        // Build the awaiter call
        let awaiter_call = IRNode::AwaiterCall {
            this_arg: Box::new(IRNode::This { captured: false }),
            generator_body: Box::new(generator_body),
        };

        // Build the function declaration/expression wrapper
        let ir_params: Vec<IRParam> = params.iter().map(|p| IRParam::new(p.as_str())).collect();

        if let Some(func_name) = name {
            IRNode::FunctionDecl {
                name: func_name,
                parameters: ir_params,
                body: vec![awaiter_call],
            }
        } else {
            IRNode::FunctionExpr {
                name: None,
                parameters: ir_params,
                body: vec![awaiter_call],
                is_expression_body: false,
                body_source_range: None,
            }
        }
    }

    /// Transform just the generator body (for use by the wrapper)
    pub fn transform_generator_body(&mut self, body_idx: NodeIndex, has_await: bool) -> IRNode {
        self.state.reset();
        self.state.has_await = has_await;
        self.helpers_needed.generator = true;

        self.build_generator_body(body_idx, has_await)
    }

    /// Build the generator body IR
    fn build_generator_body(&mut self, body_idx: NodeIndex, has_await: bool) -> IRNode {
        self.state.in_async_body = true;
        self.state.label_counter = 0;

        let cases = self.build_generator_cases(body_idx, has_await);

        self.state.in_async_body = false;

        IRNode::GeneratorBody { has_await, cases }
    }

    /// Build generator cases for the state machine
    fn build_generator_cases(
        &mut self,
        body_idx: NodeIndex,
        _has_await: bool,
    ) -> Vec<IRGeneratorCase> {
        let mut cases = Vec::new();
        let mut current_statements = Vec::new();
        let mut current_label = self.state.next_label();

        // Process the function body
        self.process_async_body(
            body_idx,
            &mut cases,
            &mut current_statements,
            &mut current_label,
        );

        // Add final case if there are remaining statements
        if !current_statements.is_empty() {
            // Only add implicit return if the last statement isn't already a return
            let needs_implicit_return =
                !matches!(current_statements.last(), Some(IRNode::ReturnStatement(_)));
            if needs_implicit_return {
                current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                    IRNode::GeneratorOp {
                        opcode: opcodes::RETURN,
                        value: None,
                        comment: Some("return".to_string()),
                    },
                ))));
            }
            cases.push(IRGeneratorCase {
                label: current_label,
                statements: current_statements,
            });
        } else if cases.is_empty() {
            // Empty async body - still need a return case
            cases.push(IRGeneratorCase {
                label: 0,
                statements: vec![IRNode::ReturnStatement(Some(Box::new(
                    IRNode::GeneratorOp {
                        opcode: opcodes::RETURN,
                        value: None,
                        comment: Some("return".to_string()),
                    },
                )))],
            });
        }

        cases
    }

    fn process_async_body(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        // Handle block statements
        if node.kind == syntax_kind_ext::BLOCK {
            if let Some(block) = self.arena.get_block(node) {
                for &stmt_idx in &block.statements.nodes {
                    self.process_async_statement(
                        stmt_idx,
                        cases,
                        current_statements,
                        current_label,
                    );
                }
            }
            return;
        }

        // Handle concise arrow body (expression)
        // For concise arrow functions like `async () => await foo()`, the body is an expression
        // not a statement. We treat this as an implicit return of the expression.
        if node.kind == syntax_kind_ext::AWAIT_EXPRESSION {
            // return await expr -> yield, then return _a.sent()
            self.process_await_expression(idx, cases, current_statements, current_label);
            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::RETURN,
                    value: Some(Box::new(IRNode::GeneratorSent)),
                    comment: Some("return".to_string()),
                },
            ))));
        } else {
            // Non-await expression body: return the expression directly
            let value = self.expression_to_ir(idx);
            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::RETURN,
                    value: Some(Box::new(value)),
                    comment: Some("return".to_string()),
                },
            ))));
        }
    }

    fn process_async_statement(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr_stmt) = self.arena.get_expression_statement(node) {
                    self.process_expression_in_async(
                        expr_stmt.expression,
                        cases,
                        current_statements,
                        current_label,
                    );
                }
            }

            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret) = self.arena.get_return_statement(node) {
                    if ret.expression.is_none() {
                        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                            IRNode::GeneratorOp {
                                opcode: opcodes::RETURN,
                                value: None,
                                comment: Some("return".to_string()),
                            },
                        ))));
                    } else if self.is_await_expression(ret.expression) {
                        // return await expr; -> yield, then return _a.sent()
                        self.process_await_expression(
                            ret.expression,
                            cases,
                            current_statements,
                            current_label,
                        );

                        // After the yield resumes, return the sent value
                        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                            IRNode::GeneratorOp {
                                opcode: opcodes::RETURN,
                                value: Some(Box::new(IRNode::GeneratorSent)),
                                comment: Some("return".to_string()),
                            },
                        ))));
                    } else {
                        let value = self.expression_to_ir(ret.expression);
                        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                            IRNode::GeneratorOp {
                                opcode: opcodes::RETURN,
                                value: Some(Box::new(value)),
                                comment: Some("return".to_string()),
                            },
                        ))));
                    }
                }
            }

            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                // Structure: VARIABLE_STATEMENT -> VARIABLE_DECLARATION_LIST -> VARIABLE_DECLARATION
                if let Some(var_stmt) = self.arena.get_variable(node) {
                    for &decl_list_idx in &var_stmt.declarations.nodes {
                        if let Some(decl_list_node) = self.arena.get(decl_list_idx) {
                            if let Some(decl_list) = self.arena.get_variable(decl_list_node) {
                                for &decl_idx in &decl_list.declarations.nodes {
                                    self.process_variable_declaration(
                                        decl_idx,
                                        cases,
                                        current_statements,
                                        current_label,
                                    );
                                }
                            }
                        }
                    }
                }
            }

            _ => {
                // Pass through other statements as-is
                let ir = self.statement_to_ir(idx);
                current_statements.push(ir);
            }
        }
    }

    fn process_expression_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        // Check for await expression
        if node.kind == syntax_kind_ext::AWAIT_EXPRESSION {
            self.process_await_expression(idx, cases, current_statements, current_label);
            // Add _a.sent() to consume the result
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::GeneratorSent)));
            return;
        }

        // For other expressions, convert to IR and add as expression statement
        let ir = self.expression_to_ir(idx);
        current_statements.push(IRNode::ExpressionStatement(Box::new(ir)));
    }

    fn process_await_expression(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        // await uses UnaryExprDataEx
        if let Some(await_expr) = self.arena.get_unary_expr_ex(node) {
            // Get the awaited expression
            let operand = self.expression_to_ir(await_expr.expression);

            // Emit: return [4 /*yield*/, operand];
            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::YIELD,
                    value: Some(Box::new(operand)),
                    comment: Some("yield".to_string()),
                },
            ))));

            // Create new case for code after await
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            *current_label = self.state.next_label();
        }
    }

    fn process_variable_declaration(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if let Some(decl) = self.arena.get_variable_declaration(node) {
            let name = self.get_identifier_text(decl.name);

            // Check if initializer contains await
            if !decl.initializer.is_none() && self.is_await_expression(decl.initializer) {
                // var x = await foo(); -> first declare var x, then yield foo(), then x = _a.sent()
                // We need to declare the variable first to avoid ReferenceError in strict mode
                current_statements.push(IRNode::VarDecl {
                    name: name.clone(),
                    initializer: None,
                });

                self.process_await_expression(
                    decl.initializer,
                    cases,
                    current_statements,
                    current_label,
                );

                // Assign the sent value to the variable
                current_statements.push(IRNode::ExpressionStatement(Box::new(
                    IRNode::BinaryExpr {
                        left: Box::new(IRNode::Identifier(name)),
                        operator: "=".to_string(),
                        right: Box::new(IRNode::GeneratorSent),
                    },
                )));
            } else if !decl.initializer.is_none() && self.contains_await_recursive(decl.initializer)
            {
                // Initializer contains await but is not a direct await expression
                // (e.g., var x = (await foo()) + 1;)
                // Declare variable first, then process
                current_statements.push(IRNode::VarDecl {
                    name: name.clone(),
                    initializer: None,
                });

                // Process the expression which may have nested awaits
                let init = self.expression_to_ir(decl.initializer);
                current_statements.push(IRNode::ExpressionStatement(Box::new(
                    IRNode::BinaryExpr {
                        left: Box::new(IRNode::Identifier(name)),
                        operator: "=".to_string(),
                        right: Box::new(init),
                    },
                )));
            } else {
                // No await in initializer - emit as normal
                let init = if decl.initializer.is_none() {
                    None
                } else {
                    Some(Box::new(self.expression_to_ir(decl.initializer)))
                };

                current_statements.push(IRNode::VarDecl {
                    name,
                    initializer: init,
                });
            }
        }
    }

    // =========================================================================
    // Helper methods
    // =========================================================================

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

        // Don't recurse into nested functions
        // This check must happen before recursing into any children
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
        // Structure: VARIABLE_STATEMENT -> VARIABLE_DECLARATION_LIST -> VARIABLE_DECLARATION
        if node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
            if let Some(var_stmt) = self.arena.get_variable(node) {
                for &decl_list_idx in &var_stmt.declarations.nodes {
                    if let Some(decl_list_node) = self.arena.get(decl_list_idx) {
                        if let Some(decl_list) = self.arena.get_variable(decl_list_node) {
                            for &decl_idx in &decl_list.declarations.nodes {
                                if let Some(decl_node) = self.arena.get(decl_idx) {
                                    if let Some(decl) =
                                        self.arena.get_variable_declaration(decl_node)
                                    {
                                        if self.contains_await_recursive(decl.initializer) {
                                            return true;
                                        }
                                    }
                                }
                            }
                        }
                    }
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

        // Check binary expressions
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            if let Some(bin) = self.arena.get_binary_expr(node) {
                return self.contains_await_recursive(bin.left)
                    || self.contains_await_recursive(bin.right);
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

        // Check array/object literals
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

        // Check loop statements
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

        // Check case blocks
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

        // Check new expressions
        if node.kind == syntax_kind_ext::NEW_EXPRESSION {
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

        // Check template expressions
        if node.kind == syntax_kind_ext::TEMPLATE_EXPRESSION {
            if let Some(template) = self.arena.get_template_expr(node) {
                for &span_idx in &template.template_spans.nodes {
                    if let Some(span_node) = self.arena.get(span_idx) {
                        if let Some(span) = self.arena.get_template_span(span_node) {
                            if self.contains_await_recursive(span.expression) {
                                return true;
                            }
                        }
                    }
                }
            }
        }

        // Check with statements (uses IfStatementData)
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

        // Check throw statements
        if node.kind == syntax_kind_ext::THROW_STATEMENT {
            if let Some(throw_data) = self.arena.get_return_statement(node) {
                if self.contains_await_recursive(throw_data.expression) {
                    return true;
                }
            }
        }

        // Check labeled statements
        if node.kind == syntax_kind_ext::LABELED_STATEMENT {
            if let Some(labeled_data) = self.arena.get_labeled_statement(node) {
                if self.contains_await_recursive(labeled_data.statement) {
                    return true;
                }
            }
        }

        false
    }

    fn computed_name_contains_await(&self, idx: NodeIndex) -> bool {
        let Some(name_node) = self.arena.get(idx) else {
            return false;
        };

        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            if let Some(computed) = self.arena.get_computed_property(name_node) {
                return self.contains_await_recursive(computed.expression);
            }
        }

        false
    }

    fn is_await_expression(&self, idx: NodeIndex) -> bool {
        if let Some(node) = self.arena.get(idx) {
            return node.kind == syntax_kind_ext::AWAIT_EXPRESSION;
        }
        false
    }

    /// Get identifier text from a node
    pub fn get_identifier_text(&self, idx: NodeIndex) -> String {
        if let Some(node) = self.arena.get(idx) {
            if node.kind == SyntaxKind::Identifier as u16 {
                if let Some(id) = self.arena.get_identifier(node) {
                    return id.escaped_text.clone();
                }
            }
        }
        String::new()
    }

    /// Collect parameter names from a parameter list
    pub fn collect_parameters(&self, params: &crate::parser::NodeList) -> Vec<String> {
        let mut result = Vec::new();
        for &param_idx in &params.nodes {
            if let Some(param_node) = self.arena.get(param_idx) {
                if let Some(param) = self.arena.get_parameter(param_node) {
                    result.push(self.get_identifier_text(param.name));
                }
            }
        }
        result
    }

    /// Convert an AST expression to IR
    pub fn expression_to_ir(&self, idx: NodeIndex) -> IRNode {
        let Some(node) = self.arena.get(idx) else {
            return IRNode::Undefined;
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
                    IRNode::StringLiteral("".to_string())
                }
            }

            k if k == SyntaxKind::TrueKeyword as u16 => IRNode::BooleanLiteral(true),
            k if k == SyntaxKind::FalseKeyword as u16 => IRNode::BooleanLiteral(false),
            k if k == SyntaxKind::NullKeyword as u16 => IRNode::NullLiteral,
            k if k == SyntaxKind::ThisKeyword as u16 => IRNode::This { captured: false },

            k if k == SyntaxKind::Identifier as u16 => {
                let text = self.get_identifier_text(idx);
                IRNode::Identifier(text)
            }

            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(node) {
                    let callee = self.expression_to_ir(call.expression);
                    let mut args = Vec::new();
                    if let Some(arg_list) = &call.arguments {
                        for &arg_idx in &arg_list.nodes {
                            args.push(self.expression_to_ir(arg_idx));
                        }
                    }
                    IRNode::CallExpr {
                        callee: Box::new(callee),
                        arguments: args,
                    }
                } else {
                    IRNode::Undefined
                }
            }

            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node) {
                    let obj = self.expression_to_ir(access.expression);
                    let prop = self.get_identifier_text(access.name_or_argument);
                    IRNode::PropertyAccess {
                        object: Box::new(obj),
                        property: prop,
                    }
                } else {
                    IRNode::Undefined
                }
            }

            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.arena.get_binary_expr(node) {
                    let left = self.expression_to_ir(bin.left);
                    let right = self.expression_to_ir(bin.right);
                    let op = self.get_operator_text(bin.operator_token);
                    IRNode::BinaryExpr {
                        left: Box::new(left),
                        operator: op,
                        right: Box::new(right),
                    }
                } else {
                    IRNode::Undefined
                }
            }

            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                if let Some(arr) = self.arena.get_literal_expr(node) {
                    let elements: Vec<IRNode> = arr
                        .elements
                        .nodes
                        .iter()
                        .map(|&idx| self.expression_to_ir(idx))
                        .collect();
                    IRNode::ArrayLiteral(elements)
                } else {
                    IRNode::ArrayLiteral(vec![])
                }
            }

            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.arena.get_parenthesized(node) {
                    IRNode::Parenthesized(Box::new(self.expression_to_ir(paren.expression)))
                } else {
                    IRNode::Undefined
                }
            }

            k if k == syntax_kind_ext::AWAIT_EXPRESSION => {
                // In expression context, await needs special handling
                // This shouldn't typically happen as awaits are processed separately
                if let Some(await_expr) = self.arena.get_unary_expr_ex(node) {
                    self.expression_to_ir(await_expr.expression)
                } else {
                    IRNode::Undefined
                }
            }

            // NEW_EXPRESSION: `new Foo(args)`
            k if k == syntax_kind_ext::NEW_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(node) {
                    let callee = self.expression_to_ir(call.expression);
                    let mut args = Vec::new();
                    if let Some(arg_list) = &call.arguments {
                        for &arg_idx in &arg_list.nodes {
                            args.push(self.expression_to_ir(arg_idx));
                        }
                    }
                    IRNode::NewExpr {
                        callee: Box::new(callee),
                        arguments: args,
                    }
                } else {
                    IRNode::Undefined
                }
            }

            // SPREAD_ELEMENT: `...expr`
            k if k == syntax_kind_ext::SPREAD_ELEMENT => {
                if let Some(spread) = self.arena.get_spread(node) {
                    IRNode::SpreadElement(Box::new(self.expression_to_ir(spread.expression)))
                } else if let Some(unary_ex) = self.arena.get_unary_expr_ex(node) {
                    // Fallback: Some spread elements use UnaryExprDataEx
                    IRNode::SpreadElement(Box::new(self.expression_to_ir(unary_ex.expression)))
                } else {
                    IRNode::Undefined
                }
            }

            // CONDITIONAL_EXPRESSION: `a ? b : c`
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = self.arena.get_conditional_expr(node) {
                    IRNode::ConditionalExpr {
                        condition: Box::new(self.expression_to_ir(cond.condition)),
                        when_true: Box::new(self.expression_to_ir(cond.when_true)),
                        when_false: Box::new(self.expression_to_ir(cond.when_false)),
                    }
                } else {
                    IRNode::Undefined
                }
            }

            // PREFIX_UNARY_EXPRESSION: `!x`, `-x`, `++x`, `--x`
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.arena.get_unary_expr(node) {
                    let op = self.get_unary_operator_text(unary.operator);
                    IRNode::PrefixUnaryExpr {
                        operator: op,
                        operand: Box::new(self.expression_to_ir(unary.operand)),
                    }
                } else {
                    IRNode::Undefined
                }
            }

            // POSTFIX_UNARY_EXPRESSION: `x++`, `x--`
            k if k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.arena.get_unary_expr(node) {
                    let op = self.get_unary_operator_text(unary.operator);
                    IRNode::PostfixUnaryExpr {
                        operand: Box::new(self.expression_to_ir(unary.operand)),
                        operator: op,
                    }
                } else {
                    IRNode::Undefined
                }
            }

            // ELEMENT_ACCESS_EXPRESSION: `object[index]`
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node) {
                    let obj = self.expression_to_ir(access.expression);
                    let index = self.expression_to_ir(access.name_or_argument);
                    IRNode::ElementAccess {
                        object: Box::new(obj),
                        index: Box::new(index),
                    }
                } else {
                    IRNode::Undefined
                }
            }

            // OBJECT_LITERAL_EXPRESSION: `{ key: value, ... }`
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                if let Some(obj) = self.arena.get_literal_expr(node) {
                    let props = self.convert_object_properties(&obj.elements.nodes);
                    IRNode::ObjectLiteral(props)
                } else {
                    IRNode::ObjectLiteral(vec![])
                }
            }

            // TEMPLATE_EXPRESSION: `hello ${name}!`
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => self.convert_template_expression(idx),

            // NoSubstitutionTemplateLiteral: `hello world`
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    // Return the text as a string literal with quotes
                    IRNode::StringLiteral(lit.text.clone())
                } else {
                    IRNode::StringLiteral("".to_string())
                }
            }

            // SuperKeyword: `super`
            k if k == SyntaxKind::SuperKeyword as u16 => IRNode::Super,

            // FUNCTION_EXPRESSION: `function foo() { ... }` or `async function() { ... }`
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => self.convert_function_expression(idx),

            // ARROW_FUNCTION: `() => { ... }` or `async () => expr`
            k if k == syntax_kind_ext::ARROW_FUNCTION => self.convert_arrow_function(idx),

            _ => IRNode::ASTRef(idx),
        }
    }

    /// Convert object literal properties to IRProperty
    fn convert_object_properties(&self, nodes: &[NodeIndex]) -> Vec<IRProperty> {
        let mut props = Vec::new();
        for &prop_idx in nodes {
            let Some(prop_node) = self.arena.get(prop_idx) else {
                continue;
            };

            match prop_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    if let Some(pa) = self.arena.get_property_assignment(prop_node) {
                        let key = self.convert_property_key(pa.name);
                        let value = self.expression_to_ir(pa.initializer);
                        props.push(IRProperty {
                            key,
                            value,
                            kind: IRPropertyKind::Init,
                        });
                    }
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    if let Some(sp) = self.arena.get_shorthand_property(prop_node) {
                        let name = self.get_identifier_text(sp.name);
                        props.push(IRProperty {
                            key: IRPropertyKey::Identifier(name.clone()),
                            value: IRNode::Identifier(name),
                            kind: IRPropertyKind::Init,
                        });
                    }
                }
                k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
                    if let Some(spread) = self.arena.get_spread(prop_node) {
                        // For spread in objects, use SpreadElement
                        props.push(IRProperty {
                            key: IRPropertyKey::Identifier("...".to_string()),
                            value: IRNode::SpreadElement(Box::new(
                                self.expression_to_ir(spread.expression),
                            )),
                            kind: IRPropertyKind::Init,
                        });
                    }
                }
                // Skip other property types (getters/setters would need special handling)
                _ => {}
            }
        }
        props
    }

    /// Convert a property name node to IRPropertyKey
    fn convert_property_key(&self, idx: NodeIndex) -> IRPropertyKey {
        let Some(node) = self.arena.get(idx) else {
            return IRPropertyKey::Identifier(String::new());
        };

        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                IRPropertyKey::Identifier(self.get_identifier_text(idx))
            }
            k if k == SyntaxKind::StringLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    IRPropertyKey::StringLiteral(lit.text.clone())
                } else {
                    IRPropertyKey::StringLiteral(String::new())
                }
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    IRPropertyKey::NumericLiteral(lit.text.clone())
                } else {
                    IRPropertyKey::NumericLiteral("0".to_string())
                }
            }
            k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                // Computed property: [expr]
                if let Some(computed) = self.arena.get_computed_property(node) {
                    IRPropertyKey::Computed(Box::new(self.expression_to_ir(computed.expression)))
                } else {
                    IRPropertyKey::Identifier(String::new())
                }
            }
            _ => IRPropertyKey::Identifier(String::new()),
        }
    }

    /// Convert a template expression to IR (concatenation of strings)
    fn convert_template_expression(&self, idx: NodeIndex) -> IRNode {
        let Some(node) = self.arena.get(idx) else {
            return IRNode::StringLiteral("".to_string());
        };

        let Some(template) = self.arena.get_template_expr(node) else {
            return IRNode::StringLiteral("".to_string());
        };

        // Get head (the initial string before first ${...})
        let mut parts: Vec<IRNode> = Vec::new();
        if let Some(head_node) = self.arena.get(template.head) {
            if let Some(lit) = self.arena.get_literal(head_node) {
                if !lit.text.is_empty() {
                    parts.push(IRNode::StringLiteral(lit.text.clone()));
                }
            }
        }

        // Process template spans (expression + literal pairs)
        for &span_idx in &template.template_spans.nodes {
            let Some(span_node) = self.arena.get(span_idx) else {
                continue;
            };
            if let Some(span) = self.arena.get_template_span(span_node) {
                // Add the expression
                parts.push(self.expression_to_ir(span.expression));

                // Add the literal part after the expression
                if let Some(lit_node) = self.arena.get(span.literal) {
                    if let Some(lit) = self.arena.get_literal(lit_node) {
                        if !lit.text.is_empty() {
                            parts.push(IRNode::StringLiteral(lit.text.clone()));
                        }
                    }
                }
            }
        }

        // If there's only one part, return it directly
        if parts.len() == 1 {
            return parts.remove(0);
        }

        // Otherwise, build a concatenation chain: part1 + part2 + part3 + ...
        if parts.is_empty() {
            return IRNode::StringLiteral("".to_string());
        }

        let mut result = parts.remove(0);
        for part in parts {
            result = IRNode::BinaryExpr {
                left: Box::new(result),
                operator: "+".to_string(),
                right: Box::new(part),
            };
        }
        result
    }

    /// Convert a function expression to IR
    fn convert_function_expression(&self, idx: NodeIndex) -> IRNode {
        let Some(node) = self.arena.get(idx) else {
            return IRNode::Undefined;
        };

        let Some(func) = self.arena.get_function(node) else {
            return IRNode::Undefined;
        };

        // Get the function name if any
        let name = if func.name.is_none() {
            None
        } else {
            Some(self.get_identifier_text(func.name))
        };

        // Convert parameters
        let params = self.convert_parameters(&func.parameters.nodes);

        // Convert body to IR statements
        let body = self.convert_function_body(func.body);

        IRNode::FunctionExpr {
            name,
            parameters: params,
            body,
            is_expression_body: false,
            body_source_range: None,
        }
    }

    /// Convert an arrow function to IR
    fn convert_arrow_function(&self, idx: NodeIndex) -> IRNode {
        let Some(node) = self.arena.get(idx) else {
            return IRNode::Undefined;
        };

        // Arrow functions also use FunctionData
        let Some(func) = self.arena.get_function(node) else {
            return IRNode::Undefined;
        };

        // Convert parameters
        let params = self.convert_parameters(&func.parameters.nodes);

        // Check if body is expression or block
        let Some(body_node) = self.arena.get(func.body) else {
            return IRNode::FunctionExpr {
                name: None,
                parameters: params,
                body: vec![],
                is_expression_body: false,
                body_source_range: None,
            };
        };

        if body_node.kind == syntax_kind_ext::BLOCK {
            // Block body
            let body = self.convert_function_body(func.body);
            IRNode::FunctionExpr {
                name: None,
                parameters: params,
                body,
                is_expression_body: false,
                body_source_range: None,
            }
        } else {
            // Expression body - wrap in return
            let expr = self.expression_to_ir(func.body);
            IRNode::FunctionExpr {
                name: None,
                parameters: params,
                body: vec![IRNode::ReturnStatement(Some(Box::new(expr)))],
                is_expression_body: true,
                body_source_range: None,
            }
        }
    }

    /// Convert function parameters to IRParam vec
    fn convert_parameters(&self, param_nodes: &[NodeIndex]) -> Vec<IRParam> {
        let mut params = Vec::new();
        for &param_idx in param_nodes {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };

            if param_node.kind == syntax_kind_ext::PARAMETER {
                if let Some(param) = self.arena.get_parameter(param_node) {
                    let name = self.get_identifier_text(param.name);
                    if param.dot_dot_dot_token {
                        params.push(IRParam::rest(name));
                    } else {
                        params.push(IRParam::new(name));
                    }
                }
            }
        }
        params
    }

    /// Convert a function body (block) to IR statements
    fn convert_function_body(&self, body_idx: NodeIndex) -> Vec<IRNode> {
        let Some(body_node) = self.arena.get(body_idx) else {
            return vec![];
        };

        if body_node.kind != syntax_kind_ext::BLOCK {
            return vec![];
        }

        let Some(block) = self.arena.get_block(body_node) else {
            return vec![];
        };

        block
            .statements
            .nodes
            .iter()
            .map(|&stmt_idx| self.statement_to_ir(stmt_idx))
            .collect()
    }

    /// Get unary operator text from a token kind
    pub fn get_unary_operator_text(&self, op: u16) -> String {
        match op {
            k if k == SyntaxKind::PlusPlusToken as u16 => "++".to_string(),
            k if k == SyntaxKind::MinusMinusToken as u16 => "--".to_string(),
            k if k == SyntaxKind::ExclamationToken as u16 => "!".to_string(),
            k if k == SyntaxKind::TildeToken as u16 => "~".to_string(),
            k if k == SyntaxKind::PlusToken as u16 => "+".to_string(),
            k if k == SyntaxKind::MinusToken as u16 => "-".to_string(),
            k if k == SyntaxKind::TypeOfKeyword as u16 => "typeof ".to_string(),
            k if k == SyntaxKind::VoidKeyword as u16 => "void ".to_string(),
            k if k == SyntaxKind::DeleteKeyword as u16 => "delete ".to_string(),
            _ => "".to_string(),
        }
    }

    /// Convert an AST statement to IR
    pub fn statement_to_ir(&self, idx: NodeIndex) -> IRNode {
        let Some(node) = self.arena.get(idx) else {
            return IRNode::EmptyStatement;
        };

        match node.kind {
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr_stmt) = self.arena.get_expression_statement(node) {
                    let expr = self.expression_to_ir(expr_stmt.expression);
                    IRNode::ExpressionStatement(Box::new(expr))
                } else {
                    IRNode::EmptyStatement
                }
            }

            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret) = self.arena.get_return_statement(node) {
                    if ret.expression.is_none() {
                        IRNode::ReturnStatement(None)
                    } else {
                        IRNode::ReturnStatement(Some(Box::new(
                            self.expression_to_ir(ret.expression),
                        )))
                    }
                } else {
                    IRNode::ReturnStatement(None)
                }
            }

            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_data) = self.arena.get_variable(node) {
                    let mut decls = Vec::new();
                    for &decl_idx in &var_data.declarations.nodes {
                        if let Some(decl_node) = self.arena.get(decl_idx) {
                            if let Some(decl) = self.arena.get_variable_declaration(decl_node) {
                                let name = self.get_identifier_text(decl.name);
                                let init = if decl.initializer.is_none() {
                                    None
                                } else {
                                    Some(Box::new(self.expression_to_ir(decl.initializer)))
                                };
                                decls.push(IRNode::VarDecl {
                                    name,
                                    initializer: init,
                                });
                            }
                        }
                    }
                    if decls.len() == 1 {
                        decls.remove(0)
                    } else {
                        IRNode::VarDeclList(decls)
                    }
                } else {
                    IRNode::EmptyStatement
                }
            }

            _ => IRNode::ASTRef(idx),
        }
    }

    /// Get operator text from a token kind
    pub fn get_operator_text(&self, op: u16) -> String {
        match op {
            k if k == SyntaxKind::PlusToken as u16 => "+".to_string(),
            k if k == SyntaxKind::MinusToken as u16 => "-".to_string(),
            k if k == SyntaxKind::AsteriskToken as u16 => "*".to_string(),
            k if k == SyntaxKind::SlashToken as u16 => "/".to_string(),
            k if k == SyntaxKind::PercentToken as u16 => "%".to_string(),
            k if k == SyntaxKind::EqualsToken as u16 => "=".to_string(),
            k if k == SyntaxKind::PlusEqualsToken as u16 => "+=".to_string(),
            k if k == SyntaxKind::MinusEqualsToken as u16 => "-=".to_string(),
            k if k == SyntaxKind::AsteriskEqualsToken as u16 => "*=".to_string(),
            k if k == SyntaxKind::SlashEqualsToken as u16 => "/=".to_string(),
            k if k == SyntaxKind::EqualsEqualsToken as u16 => "==".to_string(),
            k if k == SyntaxKind::EqualsEqualsEqualsToken as u16 => "===".to_string(),
            k if k == SyntaxKind::ExclamationEqualsToken as u16 => "!=".to_string(),
            k if k == SyntaxKind::ExclamationEqualsEqualsToken as u16 => "!==".to_string(),
            k if k == SyntaxKind::LessThanToken as u16 => "<".to_string(),
            k if k == SyntaxKind::LessThanEqualsToken as u16 => "<=".to_string(),
            k if k == SyntaxKind::GreaterThanToken as u16 => ">".to_string(),
            k if k == SyntaxKind::GreaterThanEqualsToken as u16 => ">=".to_string(),
            k if k == SyntaxKind::AmpersandAmpersandToken as u16 => "&&".to_string(),
            k if k == SyntaxKind::BarBarToken as u16 => "||".to_string(),
            _ => "?".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ParserState;
    use crate::transforms::ir_printer::IRPrinter;

    fn transform_and_print(source: &str) -> String {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        if let Some(root_node) = parser.arena.get(root)
            && let Some(source_file) = parser.arena.get_source_file(root_node)
            && let Some(&func_idx) = source_file.statements.nodes.first()
        {
            let mut transformer = AsyncES5Transformer::new(&parser.arena);
            let ir = transformer.transform_async_function(func_idx);
            IRPrinter::emit_to_string(&ir)
        } else {
            String::new()
        }
    }

    #[test]
    fn test_simple_async_function() {
        let output = transform_and_print("async function foo() { }");
        assert!(
            output.contains("function foo()"),
            "Should have function name"
        );
        assert!(output.contains("__awaiter"), "Should have awaiter call");
        assert!(
            output.contains("__generator"),
            "Should have generator wrapper"
        );
    }

    #[test]
    fn test_async_with_return() {
        let output = transform_and_print("async function foo() { return 42; }");
        assert!(output.contains("[2 /*return*/, 42]"), "Should return 42");
    }

    #[test]
    fn test_async_with_await() {
        let output = transform_and_print("async function foo() { await bar(); }");
        assert!(output.contains("switch (_a.label)"), "Should have switch");
        assert!(output.contains("[4 /*yield*/"), "Should have yield");
        assert!(output.contains("_a.sent()"), "Should call _a.sent()");
    }

    #[test]
    fn test_return_await() {
        let output = transform_and_print("async function foo() { return await bar(); }");
        assert!(output.contains("[4 /*yield*/"), "Should have yield");
        assert!(
            output.contains("[2 /*return*/, _a.sent()]"),
            "Should return _a.sent()"
        );
    }

    #[test]
    fn test_variable_with_await() {
        let output = transform_and_print("async function foo() { let x = await bar(); return x; }");
        assert!(output.contains("[4 /*yield*/"), "Should have yield");
        assert!(
            output.contains("var x;") || output.contains("var x\n"),
            "Should declare var x before assignment to avoid ReferenceError: {}",
            output
        );
        assert!(output.contains("x = _a.sent()"), "Should assign _a.sent()");
    }

    #[test]
    fn test_variable_declaration_order() {
        // Verify that variable declaration comes before the yield
        let output = transform_and_print("async function foo() { const result = await fetch(); }");
        let var_pos = output.find("var result");
        let yield_pos = output.find("[4 /*yield*/");
        assert!(
            var_pos.is_some() && yield_pos.is_some() && var_pos.unwrap() < yield_pos.unwrap(),
            "Variable declaration must come before yield: {}",
            output
        );
    }
}
