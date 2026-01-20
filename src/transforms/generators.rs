//! ES5 Generator Function Transform
//!
//! Transforms generator functions to ES5 state machines.
//!
//! # Transform Patterns
//!
//! ## Simple generator function
//! ```typescript
//! function* gen() {
//!     yield 1;
//!     yield 2;
//!     return 3;
//! }
//! ```
//! Becomes:
//! ```javascript
//! function gen() {
//!     return __generator(this, function (_a) {
//!         switch (_a.label) {
//!             case 0: return [4 /*yield*/, 1];
//!             case 1:
//!                 _a.sent();
//!                 return [4 /*yield*/, 2];
//!             case 2:
//!                 _a.sent();
//!                 return [2 /*return*/, 3];
//!         }
//!     });
//! }
//! ```
//!
//! ## Generator with yield*
//! ```typescript
//! function* delegating() {
//!     yield* [1, 2, 3];
//! }
//! ```
//! Becomes:
//! ```javascript
//! function delegating() {
//!     return __generator(this, function (_a) {
//!         switch (_a.label) {
//!             case 0: return [5 /*yield**/, __values([1, 2, 3])];
//!             case 1:
//!                 _a.sent();
//!                 return [2 /*return*/];
//!         }
//!     });
//! }
//! ```
//!
//! # Generator Opcodes
//!
//! The `__generator` helper uses these opcodes:
//! - 0: next (resume)
//! - 1: throw
//! - 2: return (with optional value)
//! - 3: break
//! - 4: yield (with value)
//! - 5: yield* (delegation)
//! - 6: catch
//! - 7: endfinally

use crate::parser::thin_node::ThinNodeArena;
use crate::parser::{NodeIndex, syntax_kind_ext};
use crate::scanner::SyntaxKind;
use crate::source_map::Mapping;
use crate::source_writer::source_position_from_offset;
use crate::transforms::helpers::HelpersNeeded;
use crate::transforms::ir::{IRGeneratorCase, IRNode, IRParam, IRSwitchCase};

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
    /// Yield a value
    pub const YIELD: u32 = 4;
    /// Yield* delegation
    pub const YIELD_STAR: u32 = 5;
    /// Catch
    pub const CATCH: u32 = 6;
    /// End finally
    pub const END_FINALLY: u32 = 7;
}

/// State for tracking generator function transformation
#[derive(Debug, Default)]
pub struct GeneratorTransformState {
    /// Current label counter for generator switch/case
    pub label_counter: u32,
    /// Stack of try/catch/finally contexts
    pub exception_blocks: Vec<ExceptionBlock>,
    /// Whether we're currently inside a generator body
    pub in_generator_body: bool,
    /// Current try-catch nesting depth
    pub try_depth: u32,
    /// Labels for break targets
    pub break_labels: Vec<(Option<String>, u32)>,
    /// Labels for continue targets
    pub continue_labels: Vec<(Option<String>, u32)>,
}

/// Represents an exception handling block (try/catch/finally)
#[derive(Debug, Clone)]
pub struct ExceptionBlock {
    /// Label for the try block start
    pub try_label: u32,
    /// Label for the catch block (if any)
    pub catch_label: Option<u32>,
    /// Label for the finally block (if any)
    pub finally_label: Option<u32>,
    /// Label for the block end
    pub end_label: u32,
    /// Catch variable name (if any)
    pub catch_variable: Option<String>,
}

impl GeneratorTransformState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset for a new generator function
    pub fn reset(&mut self) {
        self.label_counter = 0;
        self.exception_blocks.clear();
        self.in_generator_body = false;
        self.try_depth = 0;
        self.break_labels.clear();
        self.continue_labels.clear();
    }

    /// Get the next label number
    pub fn next_label(&mut self) -> u32 {
        let label = self.label_counter;
        self.label_counter += 1;
        label
    }

    /// Push a break label
    pub fn push_break_label(&mut self, name: Option<String>, label: u32) {
        self.break_labels.push((name, label));
    }

    /// Pop a break label
    pub fn pop_break_label(&mut self) {
        self.break_labels.pop();
    }

    /// Find break label by name
    pub fn find_break_label(&self, name: Option<&str>) -> Option<u32> {
        match name {
            Some(n) => self.break_labels.iter().rev()
                .find(|(label_name, _)| label_name.as_deref() == Some(n))
                .map(|(_, label)| *label),
            None => self.break_labels.last().map(|(_, label)| *label),
        }
    }

    /// Push a continue label
    pub fn push_continue_label(&mut self, name: Option<String>, label: u32) {
        self.continue_labels.push((name, label));
    }

    /// Pop a continue label
    pub fn pop_continue_label(&mut self) {
        self.continue_labels.pop();
    }

    /// Find continue label by name
    pub fn find_continue_label(&self, name: Option<&str>) -> Option<u32> {
        match name {
            Some(n) => self.continue_labels.iter().rev()
                .find(|(label_name, _)| label_name.as_deref() == Some(n))
                .map(|(_, label)| *label),
            None => self.continue_labels.last().map(|(_, label)| *label),
        }
    }

    /// Push an exception block
    pub fn push_exception_block(&mut self, block: ExceptionBlock) {
        self.exception_blocks.push(block);
        self.try_depth += 1;
    }

    /// Pop an exception block
    pub fn pop_exception_block(&mut self) -> Option<ExceptionBlock> {
        self.try_depth = self.try_depth.saturating_sub(1);
        self.exception_blocks.pop()
    }
}

/// Generator ES5 transformer for converting generator functions to state machines.
///
/// This transformer produces IR nodes only. String emission is handled by ir_printer.rs.
pub struct GeneratorES5Transformer<'a> {
    arena: &'a ThinNodeArena,
    state: GeneratorTransformState,
    helpers_needed: HelpersNeeded,
}

impl<'a> GeneratorES5Transformer<'a> {
    pub fn new(arena: &'a ThinNodeArena) -> Self {
        Self {
            arena,
            state: GeneratorTransformState::new(),
            helpers_needed: HelpersNeeded::default(),
        }
    }

    /// Transform a generator function declaration to IR
    pub fn transform_generator_function(&mut self, func_idx: NodeIndex) -> IRNode {
        self.reset();
        self.helpers_needed.generator = true;

        let Some(node) = self.arena.get(func_idx) else {
            return IRNode::Raw(String::new());
        };

        // Get function details
        let (name, params, body_idx) = if node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
            if let Some(func) = self.arena.get_function_declaration(node) {
                let name = self.get_identifier_text(func.name);
                let params = self.collect_parameters(&func.parameters);
                (Some(name), params, func.body)
            } else {
                return IRNode::Raw(String::new());
            }
        } else if node.kind == syntax_kind_ext::FUNCTION_EXPRESSION {
            if let Some(func) = self.arena.get_function_expression(node) {
                let name = if func.name.is_null() {
                    None
                } else {
                    Some(self.get_identifier_text(func.name))
                };
                let params = self.collect_parameters(&func.parameters);
                (name, params, func.body)
            } else {
                return IRNode::Raw(String::new());
            }
        } else {
            return IRNode::Raw(String::new());
        };

        // Build the generator body
        let generator_cases = self.build_generator_cases(body_idx);

        // Return IR node for the generator function
        IRNode::GeneratorFunction {
            name: name.clone(),
            parameters: params,
            generator_body: Box::new(IRNode::GeneratorBody {
                has_await: false,
                cases: generator_cases,
            }),
        }
    }

    /// Transform a generator method to IR
    pub fn transform_generator_method(&mut self, method_idx: NodeIndex, class_name: &str) -> IRNode {
        self.reset();
        self.helpers_needed.generator = true;

        let Some(node) = self.arena.get(method_idx) else {
            return IRNode::Raw(String::new());
        };

        if node.kind != syntax_kind_ext::METHOD_DECLARATION {
            return IRNode::Raw(String::new());
        }

        let Some(method) = self.arena.get_method(node) else {
            return IRNode::Raw(String::new());
        };

        let method_name = self.get_identifier_text(method.name);
        let params = self.collect_parameters(&method.parameters);
        let generator_cases = self.build_generator_cases(method.body);

        // Return IR node for the generator method
        IRNode::GeneratorMethod {
            class_name: class_name.to_string(),
            method_name,
            parameters: params,
            generator_body: Box::new(IRNode::GeneratorBody {
                has_await: false,
                cases: generator_cases,
            }),
            is_static: method.is_static,
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

    // =========================================================================
    // Generator body building
    // =========================================================================

    fn build_generator_cases(&mut self, body_idx: NodeIndex) -> Vec<IRGeneratorCase> {
        let mut cases = Vec::new();
        self.state.in_generator_body = true;
        self.state.label_counter = 0;

        // Always start with case 0
        let mut current_statements = Vec::new();
        let mut current_label = self.state.next_label();

        // Process the function body
        self.process_generator_body(body_idx, &mut cases, &mut current_statements, &mut current_label);

        // Add final case if there are remaining statements
        if !current_statements.is_empty() {
            // Add implicit return at end
            current_statements.push(IRNode::GeneratorOp {
                opcode: opcodes::RETURN,
                value: None,
                comment: Some("return".to_string()),
            });
            cases.push(IRGeneratorCase {
                label: current_label,
                statements: current_statements,
            });
        } else if cases.is_empty() {
            // Empty generator - still need a return case
            cases.push(IRGeneratorCase {
                label: 0,
                statements: vec![IRNode::GeneratorOp {
                    opcode: opcodes::RETURN,
                    value: None,
                    comment: Some("return".to_string()),
                }],
            });
        }

        self.state.in_generator_body = false;
        cases
    }

    fn process_generator_body(
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
                    self.process_generator_statement(stmt_idx, cases, current_statements, current_label);
                }
            }
            return;
        }

        // Handle single statement
        self.process_generator_statement(idx, cases, current_statements, current_label);
    }

    fn process_generator_statement(
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
                    self.process_expression_in_generator(
                        expr_stmt.expression,
                        cases,
                        current_statements,
                        current_label,
                    );
                }
            }

            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret) = self.arena.get_return_statement(node) {
                    let value = if ret.expression.is_null() {
                        None
                    } else {
                        Some(Box::new(self.expression_to_ir(ret.expression)))
                    };

                    current_statements.push(IRNode::GeneratorOp {
                        opcode: opcodes::RETURN,
                        value,
                        comment: Some("return".to_string()),
                    });
                }
            }

            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                // Process variable declarations
                if let Some(var_data) = self.arena.get_variable(node) {
                    for &decl_idx in &var_data.declarations.nodes {
                        self.process_variable_declaration(decl_idx, cases, current_statements, current_label);
                    }
                }
            }

            k if k == syntax_kind_ext::IF_STATEMENT => {
                self.process_if_statement(idx, cases, current_statements, current_label);
            }

            k if k == syntax_kind_ext::WHILE_STATEMENT => {
                self.process_while_statement(idx, cases, current_statements, current_label);
            }

            k if k == syntax_kind_ext::FOR_STATEMENT => {
                self.process_for_statement(idx, cases, current_statements, current_label);
            }

            k if k == syntax_kind_ext::FOR_OF_STATEMENT => {
                self.process_for_of_statement(idx, cases, current_statements, current_label);
            }

            k if k == syntax_kind_ext::TRY_STATEMENT => {
                self.process_try_statement(idx, cases, current_statements, current_label);
            }

            k if k == syntax_kind_ext::BREAK_STATEMENT => {
                if let Some(brk) = self.arena.get_break_statement(node) {
                    let label_name = if brk.label.is_null() {
                        None
                    } else {
                        Some(self.get_identifier_text(brk.label))
                    };
                    if let Some(target_label) = self.state.find_break_label(label_name.as_deref()) {
                        current_statements.push(IRNode::GeneratorOp {
                            opcode: opcodes::BREAK,
                            value: Some(Box::new(IRNode::NumericLiteral(target_label.to_string()))),
                            comment: Some("break".to_string()),
                        });
                    }
                }
            }

            k if k == syntax_kind_ext::CONTINUE_STATEMENT => {
                if let Some(cont) = self.arena.get_continue_statement(node) {
                    let label_name = if cont.label.is_null() {
                        None
                    } else {
                        Some(self.get_identifier_text(cont.label))
                    };
                    if let Some(target_label) = self.state.find_continue_label(label_name.as_deref()) {
                        current_statements.push(IRNode::GeneratorOp {
                            opcode: opcodes::BREAK,
                            value: Some(Box::new(IRNode::NumericLiteral(target_label.to_string()))),
                            comment: Some("continue".to_string()),
                        });
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

    fn process_expression_in_generator(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        // Check for yield expression
        if node.kind == syntax_kind_ext::YIELD_EXPRESSION {
            self.process_yield_expression(idx, cases, current_statements, current_label);
            return;
        }

        // For other expressions, convert to IR and add as expression statement
        let ir = self.expression_to_ir(idx);
        current_statements.push(IRNode::ExpressionStatement(Box::new(ir)));
    }

    fn process_yield_expression(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if let Some(yield_expr) = self.arena.get_yield_expression(node) {
            let is_delegation = yield_expr.asterisk_token;

            // Get the yielded value
            let value = if yield_expr.expression.is_null() {
                None
            } else {
                let value_ir = self.expression_to_ir(yield_expr.expression);
                if is_delegation {
                    // For yield*, wrap in __values()
                    self.helpers_needed.values = true;
                    Some(Box::new(IRNode::CallExpr {
                        callee: Box::new(IRNode::Identifier("__values".to_string())),
                        arguments: vec![value_ir],
                    }))
                } else {
                    Some(Box::new(value_ir))
                }
            };

            // Create yield operation
            let opcode = if is_delegation {
                opcodes::YIELD_STAR
            } else {
                opcodes::YIELD
            };

            current_statements.push(IRNode::GeneratorOp {
                opcode,
                value,
                comment: Some(if is_delegation { "yield*" } else { "yield" }.to_string()),
            });

            // Create new case for code after yield
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            *current_label = self.state.next_label();

            // Add _a.sent() to get the value passed back
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::GeneratorSent)));
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

            // Check if initializer contains yield
            if !decl.initializer.is_null() && self.contains_yield(decl.initializer) {
                // Split: declare variable, then assign after yield
                current_statements.push(IRNode::VarDecl {
                    name: name.clone(),
                    initializer: None,
                });

                // Process initializer (may yield)
                self.process_expression_in_generator(
                    decl.initializer,
                    cases,
                    current_statements,
                    current_label,
                );

                // Assign the sent value
                current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::BinaryExpr {
                    left: Box::new(IRNode::Identifier(name)),
                    operator: "=".to_string(),
                    right: Box::new(IRNode::GeneratorSent),
                })));
            } else {
                // No yield in initializer - emit as normal
                let init = if decl.initializer.is_null() {
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

    fn process_if_statement(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if let Some(if_stmt) = self.arena.get_if_statement(node) {
            let has_else = !if_stmt.else_statement.is_null();
            let then_contains_yield = self.contains_yield(if_stmt.then_statement);
            let else_contains_yield = has_else && self.contains_yield(if_stmt.else_statement);

            if !then_contains_yield && !else_contains_yield {
                // No yields - emit as regular if
                let ir = self.statement_to_ir(idx);
                current_statements.push(ir);
                return;
            }

            // Need to split into cases
            let end_label = self.state.next_label();
            let else_label = if has_else {
                Some(self.state.next_label())
            } else {
                None
            };

            // Emit condition check
            let condition = self.expression_to_ir(if_stmt.expression);
            let jump_label = else_label.unwrap_or(end_label);

            // if (!condition) goto else/end
            current_statements.push(IRNode::IfStatement {
                condition: Box::new(IRNode::PrefixUnaryExpr {
                    operator: "!".to_string(),
                    operand: Box::new(condition),
                }),
                then_branch: Box::new(IRNode::GeneratorOp {
                    opcode: opcodes::BREAK,
                    value: Some(Box::new(IRNode::NumericLiteral(jump_label.to_string()))),
                    comment: Some("goto else/end".to_string()),
                }),
                else_branch: None,
            });

            // Close current case
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            // Process then branch
            *current_label = self.state.next_label();
            self.process_generator_body(if_stmt.then_statement, cases, current_statements, current_label);

            // Jump to end after then
            current_statements.push(IRNode::GeneratorOp {
                opcode: opcodes::BREAK,
                value: Some(Box::new(IRNode::NumericLiteral(end_label.to_string()))),
                comment: Some("goto end".to_string()),
            });

            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            // Process else branch if present
            if let Some(else_lbl) = else_label {
                *current_label = else_lbl;
                self.process_generator_body(if_stmt.else_statement, cases, current_statements, current_label);

                cases.push(IRGeneratorCase {
                    label: *current_label,
                    statements: std::mem::take(current_statements),
                });
            }

            *current_label = end_label;
        }
    }

    fn process_while_statement(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if let Some(while_stmt) = self.arena.get_while_statement(node) {
            if !self.contains_yield(while_stmt.statement) {
                // No yield in body - emit as regular while
                let ir = self.statement_to_ir(idx);
                current_statements.push(ir);
                return;
            }

            let loop_label = self.state.next_label();
            let end_label = self.state.next_label();

            // Push break/continue labels
            self.state.push_break_label(None, end_label);
            self.state.push_continue_label(None, loop_label);

            // Close current case
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            // Loop header case
            *current_label = loop_label;

            // Check condition
            let condition = self.expression_to_ir(while_stmt.expression);
            current_statements.push(IRNode::IfStatement {
                condition: Box::new(IRNode::PrefixUnaryExpr {
                    operator: "!".to_string(),
                    operand: Box::new(condition),
                }),
                then_branch: Box::new(IRNode::GeneratorOp {
                    opcode: opcodes::BREAK,
                    value: Some(Box::new(IRNode::NumericLiteral(end_label.to_string()))),
                    comment: Some("exit while".to_string()),
                }),
                else_branch: None,
            });

            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            // Process body
            *current_label = self.state.next_label();
            self.process_generator_body(while_stmt.statement, cases, current_statements, current_label);

            // Jump back to loop header
            current_statements.push(IRNode::GeneratorOp {
                opcode: opcodes::BREAK,
                value: Some(Box::new(IRNode::NumericLiteral(loop_label.to_string()))),
                comment: Some("continue while".to_string()),
            });

            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            // Pop labels
            self.state.pop_break_label();
            self.state.pop_continue_label();

            *current_label = end_label;
        }
    }

    fn process_for_statement(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if let Some(for_stmt) = self.arena.get_for_statement(node) {
            if !self.contains_yield(for_stmt.statement) {
                let ir = self.statement_to_ir(idx);
                current_statements.push(ir);
                return;
            }

            let loop_label = self.state.next_label();
            let increment_label = self.state.next_label();
            let end_label = self.state.next_label();

            // Push break/continue labels
            self.state.push_break_label(None, end_label);
            self.state.push_continue_label(None, increment_label);

            // Emit initializer
            if !for_stmt.initializer.is_null() {
                let init_ir = self.statement_to_ir(for_stmt.initializer);
                current_statements.push(init_ir);
            }

            // Close current case
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            // Loop condition case
            *current_label = loop_label;

            if !for_stmt.condition.is_null() {
                let condition = self.expression_to_ir(for_stmt.condition);
                current_statements.push(IRNode::IfStatement {
                    condition: Box::new(IRNode::PrefixUnaryExpr {
                        operator: "!".to_string(),
                        operand: Box::new(condition),
                    }),
                    then_branch: Box::new(IRNode::GeneratorOp {
                        opcode: opcodes::BREAK,
                        value: Some(Box::new(IRNode::NumericLiteral(end_label.to_string()))),
                        comment: Some("exit for".to_string()),
                    }),
                    else_branch: None,
                });
            }

            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            // Process body
            *current_label = self.state.next_label();
            self.process_generator_body(for_stmt.statement, cases, current_statements, current_label);

            // Jump to increment
            current_statements.push(IRNode::GeneratorOp {
                opcode: opcodes::BREAK,
                value: Some(Box::new(IRNode::NumericLiteral(increment_label.to_string()))),
                comment: Some("to incrementor".to_string()),
            });

            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            // Increment case
            *current_label = increment_label;

            if !for_stmt.incrementor.is_null() {
                let incr_ir = self.expression_to_ir(for_stmt.incrementor);
                current_statements.push(IRNode::ExpressionStatement(Box::new(incr_ir)));
            }

            // Jump back to loop
            current_statements.push(IRNode::GeneratorOp {
                opcode: opcodes::BREAK,
                value: Some(Box::new(IRNode::NumericLiteral(loop_label.to_string()))),
                comment: Some("continue for".to_string()),
            });

            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            // Pop labels
            self.state.pop_break_label();
            self.state.pop_continue_label();

            *current_label = end_label;
        }
    }

    fn process_for_of_statement(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if let Some(for_of) = self.arena.get_for_of_statement(node) {
            // Mark that we need __values helper
            self.helpers_needed.values = true;

            // Get the iterator variable name
            let var_name = self.extract_for_of_variable_name(for_of.initializer);

            // Get the expression being iterated
            let iterable = self.expression_to_ir(for_of.expression);

            let loop_label = self.state.next_label();
            let end_label = self.state.next_label();

            // Push break/continue labels
            self.state.push_break_label(None, end_label);
            self.state.push_continue_label(None, loop_label);

            // Initialize iterator: var _i = __values(expression)
            current_statements.push(IRNode::VarDecl {
                name: "_i".to_string(),
                initializer: Some(Box::new(IRNode::CallExpr {
                    callee: Box::new(IRNode::Identifier("__values".to_string())),
                    arguments: vec![iterable],
                })),
            });

            // Variable for result: var _r
            current_statements.push(IRNode::VarDecl {
                name: "_r".to_string(),
                initializer: None,
            });

            // Close current case
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            // Loop case: check _r = _i.next(), !_r.done
            *current_label = loop_label;

            current_statements.push(IRNode::IfStatement {
                condition: Box::new(IRNode::PropertyAccess {
                    object: Box::new(IRNode::Parenthesized(Box::new(IRNode::BinaryExpr {
                        left: Box::new(IRNode::Identifier("_r".to_string())),
                        operator: "=".to_string(),
                        right: Box::new(IRNode::CallExpr {
                            callee: Box::new(IRNode::PropertyAccess {
                                object: Box::new(IRNode::Identifier("_i".to_string())),
                                property: "next".to_string(),
                            }),
                            arguments: vec![],
                        }),
                    }))),
                    property: "done".to_string(),
                }),
                then_branch: Box::new(IRNode::GeneratorOp {
                    opcode: opcodes::BREAK,
                    value: Some(Box::new(IRNode::NumericLiteral(end_label.to_string()))),
                    comment: Some("exit for-of".to_string()),
                }),
                else_branch: None,
            });

            // Assign value to variable
            current_statements.push(IRNode::VarDecl {
                name: var_name,
                initializer: Some(Box::new(IRNode::PropertyAccess {
                    object: Box::new(IRNode::Identifier("_r".to_string())),
                    property: "value".to_string(),
                })),
            });

            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            // Process body
            *current_label = self.state.next_label();
            self.process_generator_body(for_of.statement, cases, current_statements, current_label);

            // Jump back to loop
            current_statements.push(IRNode::GeneratorOp {
                opcode: opcodes::BREAK,
                value: Some(Box::new(IRNode::NumericLiteral(loop_label.to_string()))),
                comment: Some("continue for-of".to_string()),
            });

            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            // Pop labels
            self.state.pop_break_label();
            self.state.pop_continue_label();

            *current_label = end_label;
        }
    }

    fn process_try_statement(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if let Some(try_stmt) = self.arena.get_try_statement(node) {
            let try_label = *current_label;
            let catch_label = if !try_stmt.catch_clause.is_null() {
                Some(self.state.next_label())
            } else {
                None
            };
            let finally_label = if !try_stmt.finally_block.is_null() {
                Some(self.state.next_label())
            } else {
                None
            };
            let end_label = self.state.next_label();

            // Get catch variable name
            let catch_var = if !try_stmt.catch_clause.is_null() {
                if let Some(catch_node) = self.arena.get(try_stmt.catch_clause) {
                    if let Some(catch_clause) = self.arena.get_catch_clause(catch_node) {
                        if !catch_clause.variable_declaration.is_null() {
                            Some(self.get_identifier_text(catch_clause.variable_declaration))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            // Push exception block info
            self.state.push_exception_block(ExceptionBlock {
                try_label,
                catch_label,
                finally_label,
                end_label,
                catch_variable: catch_var.clone(),
            });

            // Process try block
            self.process_generator_body(try_stmt.try_block, cases, current_statements, current_label);

            // Jump to finally or end
            let jump_target = finally_label.unwrap_or(end_label);
            current_statements.push(IRNode::GeneratorOp {
                opcode: opcodes::BREAK,
                value: Some(Box::new(IRNode::NumericLiteral(jump_target.to_string()))),
                comment: Some("exit try".to_string()),
            });

            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            // Process catch block if present
            if let Some(catch_lbl) = catch_label {
                *current_label = catch_lbl;

                // Get exception from _.exception
                if let Some(var) = &catch_var {
                    current_statements.push(IRNode::VarDecl {
                        name: var.clone(),
                        initializer: Some(Box::new(IRNode::PropertyAccess {
                            object: Box::new(IRNode::Identifier("_".to_string())),
                            property: "exception".to_string(),
                        })),
                    });
                }

                if !try_stmt.catch_clause.is_null() {
                    if let Some(catch_node) = self.arena.get(try_stmt.catch_clause) {
                        if let Some(catch_clause) = self.arena.get_catch_clause(catch_node) {
                            self.process_generator_body(catch_clause.block, cases, current_statements, current_label);
                        }
                    }
                }

                // Jump to finally or end
                current_statements.push(IRNode::GeneratorOp {
                    opcode: opcodes::BREAK,
                    value: Some(Box::new(IRNode::NumericLiteral(jump_target.to_string()))),
                    comment: Some("exit catch".to_string()),
                });

                cases.push(IRGeneratorCase {
                    label: *current_label,
                    statements: std::mem::take(current_statements),
                });
            }

            // Process finally block if present
            if let Some(finally_lbl) = finally_label {
                *current_label = finally_lbl;

                self.process_generator_body(try_stmt.finally_block, cases, current_statements, current_label);

                // End finally
                current_statements.push(IRNode::GeneratorOp {
                    opcode: opcodes::END_FINALLY,
                    value: None,
                    comment: Some("end finally".to_string()),
                });

                cases.push(IRGeneratorCase {
                    label: *current_label,
                    statements: std::mem::take(current_statements),
                });
            }

            // Pop exception block
            self.state.pop_exception_block();

            *current_label = end_label;
        }
    }

    // =========================================================================
    // Helper methods
    // =========================================================================

    fn reset(&mut self) {
        self.state.reset();
        self.helpers_needed = HelpersNeeded::default();
    }

    fn contains_yield(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        // Check if this is a yield expression
        if node.kind == syntax_kind_ext::YIELD_EXPRESSION {
            return true;
        }

        // Don't recurse into nested functions
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
                    if self.contains_yield(stmt_idx) {
                        return true;
                    }
                }
            }
            return false;
        }

        // Check expression statements
        if node.kind == syntax_kind_ext::EXPRESSION_STATEMENT {
            if let Some(expr_stmt) = self.arena.get_expression_statement(node) {
                return self.contains_yield(expr_stmt.expression);
            }
        }

        // Check variable statements
        if node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
            if let Some(var_data) = self.arena.get_variable(node) {
                for &decl_idx in &var_data.declarations.nodes {
                    if let Some(decl_node) = self.arena.get(decl_idx) {
                        if let Some(decl) = self.arena.get_variable_declaration(decl_node) {
                            if self.contains_yield(decl.initializer) {
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
                return self.contains_yield(if_stmt.expression)
                    || self.contains_yield(if_stmt.then_statement)
                    || self.contains_yield(if_stmt.else_statement);
            }
        }

        // Check loops
        if node.kind == syntax_kind_ext::WHILE_STATEMENT {
            if let Some(while_stmt) = self.arena.get_while_statement(node) {
                return self.contains_yield(while_stmt.expression)
                    || self.contains_yield(while_stmt.statement);
            }
        }

        if node.kind == syntax_kind_ext::FOR_STATEMENT {
            if let Some(for_stmt) = self.arena.get_for_statement(node) {
                return self.contains_yield(for_stmt.initializer)
                    || self.contains_yield(for_stmt.condition)
                    || self.contains_yield(for_stmt.incrementor)
                    || self.contains_yield(for_stmt.statement);
            }
        }

        if node.kind == syntax_kind_ext::FOR_OF_STATEMENT {
            if let Some(for_of) = self.arena.get_for_of_statement(node) {
                return self.contains_yield(for_of.expression)
                    || self.contains_yield(for_of.statement);
            }
        }

        false
    }

    fn get_identifier_text(&self, idx: NodeIndex) -> String {
        if let Some(node) = self.arena.get(idx) {
            if let Some(text) = self.arena.get_identifier_text(node) {
                return text.to_string();
            }
        }
        String::new()
    }

    fn collect_parameters(&self, params: &crate::parser::NodeList) -> Vec<String> {
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

    fn extract_for_of_variable_name(&self, idx: NodeIndex) -> String {
        if let Some(node) = self.arena.get(idx) {
            if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                if let Some(decl_list) = self.arena.get_variable_declaration_list(node) {
                    if let Some(&first_decl) = decl_list.declarations.nodes.first() {
                        if let Some(decl_node) = self.arena.get(first_decl) {
                            if let Some(decl) = self.arena.get_variable_declaration(decl_node) {
                                return self.get_identifier_text(decl.name);
                            }
                        }
                    }
                }
            }
        }
        "_v".to_string()
    }

    fn expression_to_ir(&self, idx: NodeIndex) -> IRNode {
        let Some(node) = self.arena.get(idx) else {
            return IRNode::Undefined;
        };

        match node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => {
                if let Some(text) = self.arena.get_token_value(node) {
                    IRNode::NumericLiteral(text.to_string())
                } else {
                    IRNode::NumericLiteral("0".to_string())
                }
            }

            k if k == SyntaxKind::StringLiteral as u16 => {
                if let Some(text) = self.arena.get_token_value(node) {
                    IRNode::StringLiteral(text.to_string())
                } else {
                    IRNode::StringLiteral("\"\"".to_string())
                }
            }

            k if k == SyntaxKind::TrueKeyword as u16 => IRNode::BooleanLiteral(true),
            k if k == SyntaxKind::FalseKeyword as u16 => IRNode::BooleanLiteral(false),
            k if k == SyntaxKind::NullKeyword as u16 => IRNode::NullLiteral,

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
                if let Some(arr) = self.arena.get_array_literal(node) {
                    let elements: Vec<IRNode> = arr.elements.nodes
                        .iter()
                        .map(|&idx| self.expression_to_ir(idx))
                        .collect();
                    IRNode::ArrayLiteral(elements)
                } else {
                    IRNode::ArrayLiteral(vec![])
                }
            }

            _ => IRNode::ASTRef(idx),
        }
    }

    fn statement_to_ir(&self, idx: NodeIndex) -> IRNode {
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
                    if ret.expression.is_null() {
                        IRNode::ReturnStatement(None)
                    } else {
                        IRNode::ReturnStatement(Some(Box::new(self.expression_to_ir(ret.expression))))
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
                                let init = if decl.initializer.is_null() {
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

    fn get_operator_text(&self, idx: NodeIndex) -> String {
        if let Some(node) = self.arena.get(idx) {
            // Convert syntax kind to operator string
            match node.kind {
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
        } else {
            "?".to_string()
        }
    }

    // =========================================================================
    // Code emission
    // =========================================================================

    fn emit_generator_function(&mut self, name: Option<&str>, params: &[String], cases: &[IRGeneratorCase]) {
        // function name(params) {
        self.output.push_str("function ");
        if let Some(n) = name {
            self.output.push_str(n);
        }
        self.output.push('(');
        self.output.push_str(&params.join(", "));
        self.output.push_str(") {\n");

        self.indent_level += 1;
        self.emit_indent();
        self.output.push_str("return __generator(this, function (_a) {\n");

        self.indent_level += 1;
        self.emit_generator_switch(cases);
        self.indent_level -= 1;

        self.emit_indent();
        self.output.push_str("});\n");

        self.indent_level -= 1;
        self.output.push('}');
    }

    fn emit_generator_method(&mut self, class_name: &str, method_name: &str, params: &[String], cases: &[IRGeneratorCase], is_static: bool) {
        // ClassName.prototype.methodName = function(params) { ... }
        // or ClassName.methodName = function(params) { ... } for static
        self.output.push_str(class_name);
        if !is_static {
            self.output.push_str(".prototype");
        }
        self.output.push('.');
        self.output.push_str(method_name);
        self.output.push_str(" = function (");
        self.output.push_str(&params.join(", "));
        self.output.push_str(") {\n");

        self.indent_level += 1;
        self.emit_indent();
        self.output.push_str("return __generator(this, function (_a) {\n");

        self.indent_level += 1;
        self.emit_generator_switch(cases);
        self.indent_level -= 1;

        self.emit_indent();
        self.output.push_str("});\n");

        self.indent_level -= 1;
        self.emit_indent();
        self.output.push_str("};");
    }

    fn emit_generator_switch(&mut self, cases: &[IRGeneratorCase]) {
        self.emit_indent();
        self.output.push_str("switch (_a.label) {\n");

        for case in cases {
            self.indent_level += 1;
            self.emit_indent();
            self.output.push_str("case ");
            self.output.push_str(&case.label.to_string());
            self.output.push_str(":\n");

            self.indent_level += 1;
            for stmt in &case.statements {
                self.emit_ir_node(stmt);
            }
            self.indent_level -= 1;

            self.indent_level -= 1;
        }

        self.emit_indent();
        self.output.push_str("}\n");
    }

    fn emit_ir_node(&mut self, node: &IRNode) {
        match node {
            IRNode::GeneratorOp { opcode, value, comment } => {
                self.emit_indent();
                self.output.push_str("return [");
                self.output.push_str(&opcode.to_string());
                if let Some(c) = comment {
                    self.output.push_str(" /*");
                    self.output.push_str(c);
                    self.output.push_str("*/");
                }
                if let Some(v) = value {
                    self.output.push_str(", ");
                    self.emit_ir_expression(v);
                }
                self.output.push_str("];\n");
            }

            IRNode::GeneratorSent => {
                self.emit_indent();
                self.output.push_str("_a.sent();\n");
            }

            IRNode::ExpressionStatement(expr) => {
                self.emit_indent();
                self.emit_ir_expression(expr);
                self.output.push_str(";\n");
            }

            IRNode::VarDecl { name, initializer } => {
                self.emit_indent();
                self.output.push_str("var ");
                self.output.push_str(name);
                if let Some(init) = initializer {
                    self.output.push_str(" = ");
                    self.emit_ir_expression(init);
                }
                self.output.push_str(";\n");
            }

            IRNode::IfStatement { condition, then_branch, else_branch } => {
                self.emit_indent();
                self.output.push_str("if (");
                self.emit_ir_expression(condition);
                self.output.push_str(") ");
                self.emit_ir_node(then_branch);
                if let Some(else_br) = else_branch {
                    self.output.push_str(" else ");
                    self.emit_ir_node(else_br);
                }
            }

            IRNode::Block(stmts) => {
                self.output.push_str("{\n");
                self.indent_level += 1;
                for stmt in stmts {
                    self.emit_ir_node(stmt);
                }
                self.indent_level -= 1;
                self.emit_indent();
                self.output.push_str("}\n");
            }

            IRNode::ReturnStatement(expr) => {
                self.emit_indent();
                self.output.push_str("return");
                if let Some(e) = expr {
                    self.output.push(' ');
                    self.emit_ir_expression(e);
                }
                self.output.push_str(";\n");
            }

            _ => {
                self.emit_indent();
                self.output.push_str("/* unhandled IR node */\n");
            }
        }
    }

    fn emit_ir_expression(&mut self, node: &IRNode) {
        match node {
            IRNode::Identifier(name) => self.output.push_str(name),
            IRNode::NumericLiteral(n) => self.output.push_str(n),
            IRNode::StringLiteral(s) => self.output.push_str(s),
            IRNode::BooleanLiteral(b) => self.output.push_str(if *b { "true" } else { "false" }),
            IRNode::NullLiteral => self.output.push_str("null"),
            IRNode::Undefined => self.output.push_str("void 0"),

            IRNode::CallExpr { callee, arguments } => {
                self.emit_ir_expression(callee);
                self.output.push('(');
                for (i, arg) in arguments.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.emit_ir_expression(arg);
                }
                self.output.push(')');
            }

            IRNode::PropertyAccess { object, property } => {
                self.emit_ir_expression(object);
                self.output.push('.');
                self.output.push_str(property);
            }

            IRNode::BinaryExpr { left, operator, right } => {
                self.emit_ir_expression(left);
                self.output.push(' ');
                self.output.push_str(operator);
                self.output.push(' ');
                self.emit_ir_expression(right);
            }

            IRNode::PrefixUnaryExpr { operator, operand } => {
                self.output.push_str(operator);
                self.emit_ir_expression(operand);
            }

            IRNode::Parenthesized(inner) => {
                self.output.push('(');
                self.emit_ir_expression(inner);
                self.output.push(')');
            }

            IRNode::ArrayLiteral(elements) => {
                self.output.push('[');
                for (i, elem) in elements.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.emit_ir_expression(elem);
                }
                self.output.push(']');
            }

            IRNode::GeneratorSent => {
                self.output.push_str("_a.sent()");
            }

            _ => {
                self.output.push_str("/* expr */");
            }
        }
    }

    fn emit_indent(&mut self) {
        for _ in 0..self.indent_level {
            self.output.push_str("    ");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::thin_parser::ThinParserState;

    #[test]
    fn test_simple_generator_transform() {
        let source = r#"function* gen() { yield 1; yield 2; return 3; }"#;
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        // Navigate to function declaration
        if let Some(root_node) = parser.get_arena().get(root) {
            if let Some(sf_data) = parser.get_arena().get_source_file(root_node) {
                if let Some(&func_idx) = sf_data.statements.nodes.first() {
                    let mut transformer = GeneratorES5Transformer::new(parser.get_arena());
                    let result = transformer.transform_generator_function(func_idx);

                    // Verify output contains generator structure
                    assert!(result.contains("__generator"));
                    assert!(result.contains("switch"));
                    assert!(result.contains("case 0"));
                    assert!(transformer.helpers_needed.generator);
                }
            }
        }
    }

    #[test]
    fn test_generator_with_yield_star() {
        let source = r#"function* delegating() { yield* [1, 2, 3]; }"#;
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        if let Some(root_node) = parser.get_arena().get(root) {
            if let Some(sf_data) = parser.get_arena().get_source_file(root_node) {
                if let Some(&func_idx) = sf_data.statements.nodes.first() {
                    let mut transformer = GeneratorES5Transformer::new(parser.get_arena());
                    let result = transformer.transform_generator_function(func_idx);

                    // Verify yield* uses __values helper
                    assert!(result.contains("__generator"));
                    assert!(transformer.helpers_needed.generator);
                    assert!(transformer.helpers_needed.values);
                }
            }
        }
    }

    #[test]
    fn test_empty_generator() {
        let source = r#"function* empty() { }"#;
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        if let Some(root_node) = parser.get_arena().get(root) {
            if let Some(sf_data) = parser.get_arena().get_source_file(root_node) {
                if let Some(&func_idx) = sf_data.statements.nodes.first() {
                    let mut transformer = GeneratorES5Transformer::new(parser.get_arena());
                    let result = transformer.transform_generator_function(func_idx);

                    // Empty generator should still have return
                    assert!(result.contains("return [2"));
                }
            }
        }
    }

    #[test]
    fn test_generator_state_labels() {
        let mut state = GeneratorTransformState::new();

        // Test label allocation
        assert_eq!(state.next_label(), 0);
        assert_eq!(state.next_label(), 1);
        assert_eq!(state.next_label(), 2);

        // Test break/continue labels
        state.push_break_label(None, 10);
        state.push_break_label(Some("outer".to_string()), 20);

        assert_eq!(state.find_break_label(None), Some(20));
        assert_eq!(state.find_break_label(Some("outer")), Some(20));

        state.pop_break_label();
        assert_eq!(state.find_break_label(None), Some(10));

        // Test reset
        state.reset();
        assert_eq!(state.next_label(), 0);
        assert_eq!(state.find_break_label(None), None);
    }
}
