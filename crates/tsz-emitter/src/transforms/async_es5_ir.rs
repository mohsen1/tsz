//! ES5 Async Function Transform (IR-based)
//!
//! Transforms async functions to ES5 generators wrapped in __awaiter.
//! This module produces IR nodes that are then printed by `IRPrinter`.
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

use crate::transforms::helpers::HelpersNeeded;
use crate::transforms::ir::{IRGeneratorCase, IRNode, IRParam};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

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
    pub const fn reset(&mut self) {
        self.label_counter = 0;
        self.in_async_body = false;
        self.has_await = false;
    }

    /// Get the next label number
    pub const fn next_label(&mut self) -> u32 {
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
/// This transformer mirrors the `GeneratorES5Transformer` pattern from generators.rs.
/// It converts async functions to ES5 code using __awaiter and __generator helpers.
pub struct AsyncES5Transformer<'a> {
    pub(crate) arena: &'a NodeArena,
    source_text: Option<&'a str>,
    state: AsyncTransformState,
    helpers_needed: HelpersNeeded,
}

impl<'a> AsyncES5Transformer<'a> {
    /// Create a new `AsyncES5Transformer`
    pub fn new(arena: &'a NodeArena) -> Self {
        Self {
            arena,
            source_text: None,
            state: AsyncTransformState::new(),
            helpers_needed: HelpersNeeded::default(),
        }
    }

    pub const fn set_source_text(&mut self, source_text: &'a str) {
        self.source_text = Some(source_text);
    }

    /// Get the helpers needed after transformation
    pub const fn get_helpers_needed(&self) -> &HelpersNeeded {
        &self.helpers_needed
    }

    /// Take the helpers needed (consumes the transformer)
    pub const fn take_helpers_needed(self) -> HelpersNeeded {
        self.helpers_needed
    }

    /// Transform an async function declaration to IR
    ///
    /// Returns an `IRNode::AwaiterCall` with a nested `IRNode::GeneratorBody`
    pub fn transform_async_function(&mut self, func_idx: NodeIndex) -> IRNode {
        self.state.reset();
        self.helpers_needed.awaiter = true;
        self.helpers_needed.generator = true;

        let Some(node) = self.arena.get(func_idx) else {
            return IRNode::Undefined;
        };

        // Get function details - all function types use FunctionData
        let (name, params, body_idx, await_default_param_name, recover_await_default) = if node.kind
            == syntax_kind_ext::FUNCTION_DECLARATION
            || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || node.kind == syntax_kind_ext::ARROW_FUNCTION
        {
            if let Some(func) = self.arena.get_function(node) {
                let name = if func.name.is_none() {
                    None
                } else {
                    Some(crate::transforms::emit_utils::identifier_text_or_empty(
                        self.arena, func.name,
                    ))
                };
                let params = self.collect_parameters(&func.parameters);
                let await_default_param_name =
                    self.first_await_default_param_name(&func.parameters);
                let recover_await_default =
                    super::emit_utils::block_is_empty(self.arena, func.body)
                        && await_default_param_name.is_some()
                        && func
                            .parameters
                            .nodes
                            .iter()
                            .copied()
                            .any(|p| self.param_initializer_has_top_level_await(p));
                (
                    name,
                    params,
                    func.body,
                    await_default_param_name,
                    recover_await_default,
                )
            } else {
                return IRNode::Undefined;
            }
        } else {
            return IRNode::Undefined;
        };

        // Check if body contains await
        let has_await = self.body_contains_await(body_idx);
        self.state.has_await = has_await;

        if recover_await_default {
            let mut generated = String::new();
            generated.push_str("return __awaiter(this, arguments, void 0, function (");
            generated.push_str(&params.join(", "));
            generated.push_str(") {\n");
            if let Some(param_name) = await_default_param_name {
                generated.push_str("    if (");
                generated.push_str(&param_name);
                generated.push_str(" === void 0) { ");
                generated.push_str(&param_name);
                generated.push_str(" = _a.sent(); }\n");
            }
            generated.push_str("    return __generator(this, function (_a) {\n");
            generated.push_str("        switch (_a.label) {\n");
            generated.push_str("            case 0: return [4 /*yield*/, ];\n");
            generated.push_str("            case 1: return [2 /*return*/];\n");
            generated.push_str("        }\n");
            generated.push_str("    });\n");
            generated.push_str("});");

            if let Some(func_name) = name {
                return IRNode::FunctionDecl {
                    name: func_name,
                    parameters: Vec::new(),
                    body: vec![IRNode::Raw(generated)],
                    body_source_range: None,
                };
            }
            return IRNode::FunctionExpr {
                name: None,
                parameters: Vec::new(),
                body: vec![IRNode::Raw(generated)],
                is_expression_body: false,
                body_source_range: None,
            };
        }

        let mut hoisted_decls = Vec::new();
        let mut skipped_statements = Vec::new();
        if !has_await
            && let Some(body_node) = self.arena.get(body_idx)
            && body_node.kind == syntax_kind_ext::BLOCK
            && let Some(block) = self.arena.get_block(body_node)
        {
            for &stmt_idx in &block.statements.nodes {
                let Some(stmt_node) = self.arena.get(stmt_idx) else {
                    continue;
                };
                if stmt_node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
                    continue;
                }
                if let Some(comment) = self.extract_preceding_line_comment(stmt_node.pos) {
                    hoisted_decls.push(IRNode::Raw(comment));
                }
                skipped_statements.push(stmt_idx);
                if let Some(func) = self.arena.get_function(stmt_node) {
                    if func.is_async {
                        hoisted_decls.push(self.transform_async_function(stmt_idx));
                    } else {
                        hoisted_decls.push(IRNode::ASTRef(stmt_idx));
                    }
                } else {
                    hoisted_decls.push(IRNode::ASTRef(stmt_idx));
                }
            }
        }

        // Build the generator body
        let generator_body = self.build_generator_body(body_idx, has_await, &skipped_statements);

        // Build the awaiter call
        let awaiter_call = IRNode::AwaiterCall {
            this_arg: Box::new(IRNode::This { captured: false }),
            generator_body: Box::new(generator_body),
        };

        // Build the function declaration/expression wrapper
        let ir_params: Vec<IRParam> = params.iter().map(|p| IRParam::new(p.as_str())).collect();

        if let Some(func_name) = name {
            let mut body = hoisted_decls;
            body.push(awaiter_call);
            IRNode::FunctionDecl {
                name: func_name,
                parameters: ir_params,
                body,
                body_source_range: None,
            }
        } else {
            let mut body = hoisted_decls;
            body.push(awaiter_call);
            IRNode::FunctionExpr {
                name: None,
                parameters: ir_params,
                body,
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

        self.build_generator_body(body_idx, has_await, &[])
    }

    /// Build the generator body IR
    fn build_generator_body(
        &mut self,
        body_idx: NodeIndex,
        has_await: bool,
        skipped_statements: &[NodeIndex],
    ) -> IRNode {
        self.state.in_async_body = true;
        self.state.label_counter = 0;

        let cases = self.build_generator_cases(body_idx, has_await, skipped_statements);

        self.state.in_async_body = false;

        IRNode::GeneratorBody { has_await, cases }
    }

    /// Build generator cases for the state machine
    fn build_generator_cases(
        &mut self,
        body_idx: NodeIndex,
        _has_await: bool,
        skipped_statements: &[NodeIndex],
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
            skipped_statements,
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
        skipped_statements: &[NodeIndex],
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        // Handle block statements
        if node.kind == syntax_kind_ext::BLOCK {
            if let Some(block) = self.arena.get_block(node) {
                for &stmt_idx in &block.statements.nodes {
                    if skipped_statements.contains(&stmt_idx) {
                        continue;
                    }
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
                    } else if super::emit_utils::is_await_expression(self.arena, ret.expression) {
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
                        if let Some(decl_list_node) = self.arena.get(decl_list_idx)
                            && let Some(decl_list) = self.arena.get_variable(decl_list_node)
                        {
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
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = self.arena.get_function(node) {
                    if func.is_async {
                        // Nested async function declarations inside async bodies must be
                        // lowered as standalone functions in the generator case block.
                        current_statements.push(self.transform_async_function(idx));
                    } else {
                        current_statements.push(IRNode::ASTRef(idx));
                    }
                } else {
                    current_statements.push(IRNode::ASTRef(idx));
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
            let name =
                crate::transforms::emit_utils::identifier_text_or_empty(self.arena, decl.name);

            // Check if initializer contains await
            if decl.initializer.is_some()
                && super::emit_utils::is_await_expression(self.arena, decl.initializer)
            {
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
            } else if decl.initializer.is_some() && self.contains_await_recursive(decl.initializer)
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
        if node.kind == syntax_kind_ext::EXPRESSION_STATEMENT
            && let Some(expr_stmt) = self.arena.get_expression_statement(node)
        {
            return self.contains_await_recursive(expr_stmt.expression);
        }

        // Check return statements
        if node.kind == syntax_kind_ext::RETURN_STATEMENT
            && let Some(ret) = self.arena.get_return_statement(node)
        {
            return self.contains_await_recursive(ret.expression);
        }

        // Check variable statements
        // Structure: VARIABLE_STATEMENT -> VARIABLE_DECLARATION_LIST -> VARIABLE_DECLARATION
        if node.kind == syntax_kind_ext::VARIABLE_STATEMENT
            && let Some(var_stmt) = self.arena.get_variable(node)
        {
            for &decl_list_idx in &var_stmt.declarations.nodes {
                if let Some(decl_list_node) = self.arena.get(decl_list_idx)
                    && let Some(decl_list) = self.arena.get_variable(decl_list_node)
                {
                    for &decl_idx in &decl_list.declarations.nodes {
                        if let Some(decl_node) = self.arena.get(decl_idx)
                            && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                            && self.contains_await_recursive(decl.initializer)
                        {
                            return true;
                        }
                    }
                }
            }
        }

        // Check call expressions
        if node.kind == syntax_kind_ext::CALL_EXPRESSION
            && let Some(call) = self.arena.get_call_expr(node)
        {
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

        // Check binary expressions
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin) = self.arena.get_binary_expr(node)
        {
            return self.contains_await_recursive(bin.left)
                || self.contains_await_recursive(bin.right);
        }

        // Check if statements
        if node.kind == syntax_kind_ext::IF_STATEMENT
            && let Some(if_stmt) = self.arena.get_if_statement(node)
        {
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

        // Check property/element access expressions
        if (node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            && let Some(access) = self.arena.get_access_expr(node)
        {
            if self.contains_await_recursive(access.expression) {
                return true;
            }
            if self.contains_await_recursive(access.name_or_argument) {
                return true;
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
                            if let Some(spread) = self.arena.get_unary_expr_ex(elem_node)
                                && self.contains_await_recursive(spread.expression)
                            {
                                return true;
                            }
                        }
                        syntax_kind_ext::METHOD_DECLARATION => {
                            if let Some(method) = self.arena.get_method_decl(elem_node)
                                && self.computed_name_contains_await(method.name)
                            {
                                return true;
                            }
                        }
                        syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                            if let Some(accessor) = self.arena.get_accessor(elem_node)
                                && self.computed_name_contains_await(accessor.name)
                            {
                                return true;
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
        if node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
            && let Some(cond) = self.arena.get_conditional_expr(node)
        {
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

        // Check prefix/postfix unary expressions
        if (node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION)
            && let Some(unary) = self.arena.get_unary_expr(node)
        {
            return self.contains_await_recursive(unary.operand);
        }

        // Check parenthesized expressions
        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.arena.get_parenthesized(node)
        {
            return self.contains_await_recursive(paren.expression);
        }

        // Check try/catch/finally statements
        if node.kind == syntax_kind_ext::TRY_STATEMENT
            && let Some(try_data) = self.arena.get_try(node)
        {
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

        // Check catch clauses
        if node.kind == syntax_kind_ext::CATCH_CLAUSE
            && let Some(catch) = self.arena.get_catch_clause(node)
        {
            return self.contains_await_recursive(catch.block);
        }

        // Check loop statements
        if (node.kind == syntax_kind_ext::WHILE_STATEMENT
            || node.kind == syntax_kind_ext::DO_STATEMENT
            || node.kind == syntax_kind_ext::FOR_STATEMENT)
            && let Some(loop_data) = self.arena.get_loop(node)
        {
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

        // Check for-in/for-of statements
        if (node.kind == syntax_kind_ext::FOR_IN_STATEMENT
            || node.kind == syntax_kind_ext::FOR_OF_STATEMENT)
            && let Some(for_data) = self.arena.get_for_in_of(node)
        {
            if self.contains_await_recursive(for_data.expression) {
                return true;
            }
            if self.contains_await_recursive(for_data.statement) {
                return true;
            }
        }

        // Check switch statements
        if node.kind == syntax_kind_ext::SWITCH_STATEMENT
            && let Some(switch_data) = self.arena.get_switch(node)
        {
            if self.contains_await_recursive(switch_data.expression) {
                return true;
            }
            if self.contains_await_recursive(switch_data.case_block) {
                return true;
            }
        }

        // Check case blocks
        if node.kind == syntax_kind_ext::CASE_BLOCK
            && let Some(block_data) = self.arena.get_block(node)
        {
            for &stmt_idx in &block_data.statements.nodes {
                if self.contains_await_recursive(stmt_idx) {
                    return true;
                }
            }
        }

        // Check case/default clauses
        if (node.kind == syntax_kind_ext::CASE_CLAUSE
            || node.kind == syntax_kind_ext::DEFAULT_CLAUSE)
            && let Some(clause_data) = self.arena.get_case_clause(node)
        {
            if self.contains_await_recursive(clause_data.expression) {
                return true;
            }
            for &stmt_idx in &clause_data.statements.nodes {
                if self.contains_await_recursive(stmt_idx) {
                    return true;
                }
            }
        }

        // Check new expressions
        if node.kind == syntax_kind_ext::NEW_EXPRESSION
            && let Some(call) = self.arena.get_call_expr(node)
        {
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

        // Check template expressions
        if node.kind == syntax_kind_ext::TEMPLATE_EXPRESSION
            && let Some(template) = self.arena.get_template_expr(node)
        {
            for &span_idx in &template.template_spans.nodes {
                if let Some(span_node) = self.arena.get(span_idx)
                    && let Some(span) = self.arena.get_template_span(span_node)
                    && self.contains_await_recursive(span.expression)
                {
                    return true;
                }
            }
        }

        // Check with statements (uses IfStatementData)
        if node.kind == syntax_kind_ext::WITH_STATEMENT
            && let Some(with_data) = self.arena.get_with_statement(node)
        {
            if self.contains_await_recursive(with_data.expression) {
                return true;
            }
            if self.contains_await_recursive(with_data.then_statement) {
                return true;
            }
        }

        // Check throw statements
        if node.kind == syntax_kind_ext::THROW_STATEMENT
            && let Some(throw_data) = self.arena.get_return_statement(node)
            && self.contains_await_recursive(throw_data.expression)
        {
            return true;
        }

        // Check labeled statements
        if node.kind == syntax_kind_ext::LABELED_STATEMENT
            && let Some(labeled_data) = self.arena.get_labeled_statement(node)
            && self.contains_await_recursive(labeled_data.statement)
        {
            return true;
        }

        false
    }

    fn computed_name_contains_await(&self, idx: NodeIndex) -> bool {
        let Some(name_node) = self.arena.get(idx) else {
            return false;
        };

        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && let Some(computed) = self.arena.get_computed_property(name_node)
        {
            return self.contains_await_recursive(computed.expression);
        }

        false
    }

    fn param_initializer_has_top_level_await(&self, param_idx: NodeIndex) -> bool {
        let Some(param_node) = self.arena.get(param_idx) else {
            return false;
        };
        let Some(param) = self.arena.get_parameter(param_node) else {
            return false;
        };
        if param.initializer.is_none() {
            return false;
        }
        let Some(init_node) = self.arena.get(param.initializer) else {
            return false;
        };
        init_node.kind == syntax_kind_ext::AWAIT_EXPRESSION
    }

    fn first_await_default_param_name(
        &self,
        params: &tsz_parser::parser::NodeList,
    ) -> Option<String> {
        for &param_idx in &params.nodes {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            if param.initializer.is_none() {
                continue;
            }
            let Some(init_node) = self.arena.get(param.initializer) else {
                continue;
            };
            if init_node.kind != syntax_kind_ext::AWAIT_EXPRESSION {
                continue;
            }
            let Some(name_node) = self.arena.get(param.name) else {
                continue;
            };
            if name_node.kind != SyntaxKind::Identifier as u16 {
                continue;
            }
            let name =
                crate::transforms::emit_utils::identifier_text_or_empty(self.arena, param.name);
            if !name.is_empty() {
                return Some(name);
            }
        }
        None
    }

    fn extract_preceding_line_comment(&self, pos: u32) -> Option<String> {
        let text = self.source_text?;
        let bytes = text.as_bytes();
        let mut pos = pos as usize;
        if pos > bytes.len() {
            pos = bytes.len();
        }
        if pos == 0 {
            return None;
        }

        let line_start = text[..pos].rfind('\n').map_or(0, |i| i + 1);
        if line_start == 0 {
            return None;
        }
        let prev_line_end = line_start.saturating_sub(1);
        let prev_line_start = text[..prev_line_end].rfind('\n').map_or(0, |i| i + 1);
        let prev_line = &text[prev_line_start..prev_line_end];
        let trimmed = prev_line.trim_start();
        if trimmed.starts_with("//") && !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
        None
    }
}

#[cfg(test)]
#[path = "../../tests/async_es5_ir.rs"]
mod tests;
