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

use std::cell::Cell;

use crate::transforms::es5::ES5ClassTransformer;
use crate::transforms::helpers::HelpersNeeded;
use crate::transforms::ir::{IRGeneratorCase, IRNode, IRParam};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;

#[path = "async_es5_ir_bindings.rs"]
mod bindings;

/// State for tracking async function transformation
#[derive(Debug, Default)]
pub struct AsyncTransformState {
    /// Current label counter for generator switch/case
    pub label_counter: u32,
    /// Whether we're currently inside an async function body
    pub in_async_body: bool,
    /// Whether any await expressions were found (determines if we need switch/case)
    pub has_await: bool,
    /// Whether the body references `arguments` (needs `var arguments_1 = arguments;`)
    pub captures_arguments: bool,
    /// Generated name used for captured `arguments` references.
    pub arguments_capture_name: String,
}

enum SuspendedAssignmentTarget {
    Property(String),
    Element(Box<IRNode>),
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
        self.captures_arguments = false;
        self.arguments_capture_name.clear();
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

/// Pieces of an ES5 class factory broken out from a transformed
/// `ES5ClassIIFE` so that callers can splice the body into a generator
/// case while still emitting weakmap declarations / instantiations and
/// deferred static blocks alongside the class assignment.
struct ES5ClassFactoryParts {
    factory: IRNode,
    /// Names of `WeakMap` declarations for private fields. Must be
    /// declared as part of the surrounding scope (otherwise references
    /// to them in the class body fail at runtime with `ReferenceError`).
    weakmap_decls: Vec<String>,
    /// Pre-rendered `WeakMap` instantiation expression strings (e.g.
    /// `_value = new WeakMap()`). Emitted after the class assignment.
    weakmap_inits: Vec<String>,
    /// Static block IIFEs deferred to after the class assignment.
    deferred_static_blocks: Vec<IRNode>,
}

/// Async ES5 transformer that produces IR nodes instead of strings.
///
/// This transformer mirrors the `GeneratorES5Transformer` pattern from generators.rs.
/// It converts async functions to ES5 code using __awaiter and __generator helpers.
pub struct AsyncES5Transformer<'a> {
    pub(crate) arena: &'a NodeArena,
    pub(super) source_text: Option<&'a str>,
    pub(crate) state: AsyncTransformState,
    helpers_needed: HelpersNeeded,
    /// When true, looks for yield instead of await.
    pub(crate) generator_mode: bool,
    /// When true, generator-mode yields feed `__await(...)` values to
    /// `__asyncGenerator`.
    pub(crate) async_generator_mode: bool,
    temp_var_counter: Cell<u32>,
    lexical_this_capture: Cell<bool>,
    capture_this_references: Cell<bool>,
    loop_exit_placeholder_counter: Cell<u32>,
    /// Whether this async body is emitted inside a derived ES5 class method.
    pub(super) class_has_super: bool,
    /// Generated super parameter name for the surrounding ES5 class IIFE.
    pub(super) class_super_name: String,
    /// Whether the surrounding class member is static.
    pub(super) class_super_is_static: bool,
}

impl<'a> AsyncES5Transformer<'a> {
    /// Create a new `AsyncES5Transformer`
    pub fn new(arena: &'a NodeArena) -> Self {
        Self {
            arena,
            source_text: None,
            state: AsyncTransformState::new(),
            helpers_needed: HelpersNeeded::default(),
            generator_mode: false,
            async_generator_mode: false,
            temp_var_counter: Cell::new(0),
            lexical_this_capture: Cell::new(false),
            capture_this_references: Cell::new(false),
            loop_exit_placeholder_counter: Cell::new(0),
            class_has_super: false,
            class_super_name: "_super".to_string(),
            class_super_is_static: false,
        }
    }

    pub const fn set_source_text(&mut self, source_text: &'a str) {
        self.source_text = Some(source_text);
    }

    pub fn with_class_super_context(
        mut self,
        has_super: bool,
        super_name: String,
        is_static: bool,
    ) -> Self {
        self.class_has_super = has_super;
        self.class_super_name = super_name;
        self.class_super_is_static = is_static;
        self
    }

    pub(crate) fn set_lexical_this_capture(&self, capture: bool) {
        self.lexical_this_capture.set(capture);
    }

    pub(super) const fn captures_lexical_this(&self) -> bool {
        self.lexical_this_capture.get()
    }

    pub(super) const fn captures_this_references(&self) -> bool {
        self.capture_this_references.get()
    }

    pub(super) fn set_capture_this_references(&self, capture: bool) {
        self.capture_this_references.set(capture);
    }

    fn reset_loop_exit_placeholders(&self) {
        self.loop_exit_placeholder_counter.set(0);
    }

    fn next_loop_exit_placeholder(&self) -> u32 {
        let counter = self.loop_exit_placeholder_counter.get();
        self.loop_exit_placeholder_counter.set(counter + 1);
        u32::MAX - counter
    }

    pub(super) fn generate_hoisted_temp(&self) -> String {
        let counter = self.temp_var_counter.get();
        let name = if counter < 26 {
            format!("_{}", (b'a' + counter as u8) as char)
        } else {
            format!("_{counter}")
        };
        self.temp_var_counter.set(counter + 1);
        name
    }

    pub(super) fn set_temp_var_counter(&self, counter: u32) {
        self.temp_var_counter.set(counter);
    }

    pub const fn temp_var_counter(&self) -> u32 {
        self.temp_var_counter.get()
    }

    /// Get the helpers needed after transformation
    pub(crate) const fn suspension_kind(&self) -> u16 {
        if self.generator_mode {
            syntax_kind_ext::YIELD_EXPRESSION
        } else {
            syntax_kind_ext::AWAIT_EXPRESSION
        }
    }

    pub(crate) fn is_suspension_expression(&self, idx: NodeIndex) -> bool {
        self.arena.get(idx).is_some_and(|n| {
            n.kind == self.suspension_kind()
                || (self.async_generator_mode && n.kind == syntax_kind_ext::AWAIT_EXPRESSION)
        })
    }

    pub const fn get_helpers_needed(&self) -> &HelpersNeeded {
        &self.helpers_needed
    }

    /// Take the helpers needed (consumes the transformer)
    pub fn take_helpers_needed(self) -> HelpersNeeded {
        self.helpers_needed
    }

    /// Transform an async function declaration to IR
    ///
    /// Returns an `IRNode::AwaiterCall` with a nested `IRNode::GeneratorBody`
    pub fn transform_async_function(&mut self, func_idx: NodeIndex) -> IRNode {
        self.state.reset();
        self.reset_loop_exit_placeholders();
        self.helpers_needed.awaiter = true;
        self.helpers_needed.generator = true;

        let Some(node) = self.arena.get(func_idx) else {
            return IRNode::Undefined;
        };

        // Get function details - all function types use FunctionData
        let (
            name,
            params,
            param_binding_names,
            body_idx,
            await_default_param_name,
            recover_await_default,
            type_annotation,
        ) = if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            || node.is_function_expression_or_arrow()
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
                let mut param_binding_names = Vec::new();
                self.collect_parameter_binding_names(&func.parameters, &mut param_binding_names);
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
                    param_binding_names,
                    func.body,
                    await_default_param_name,
                    recover_await_default,
                    func.type_annotation,
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

        // Check if body references `arguments`
        let captures_arguments =
            tsz_parser::syntax::transform_utils::contains_arguments_reference(self.arena, body_idx);
        self.state.captures_arguments = captures_arguments;
        if captures_arguments {
            self.state.arguments_capture_name =
                self.fresh_arguments_capture_name(body_idx, &param_binding_names);
        }

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
                    name: func_name.into(),
                    parameters: Vec::new(),
                    body: vec![IRNode::Raw(generated.into())],
                    body_source_range: None,
                    leading_comment: None,
                };
            }
            return IRNode::FunctionExpr {
                name: None,
                parameters: Vec::new(),
                body: vec![IRNode::Raw(generated.into())],
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
                    hoisted_decls.push(IRNode::Raw(comment.into()));
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
        let mut generator_body =
            self.build_generator_body(body_idx, has_await, &skipped_statements);

        // Hoist var declarations from generator cases to the awaiter wrapper scope.
        // In tsc output, var declarations inside async function bodies are placed
        // before `return __generator(...)`, not inside the switch/case statements.
        let hoisted_var_groups = Self::extract_and_remove_var_decl_groups(&mut generator_body);

        // Extract promise constructor from return type annotation
        let promise_constructor = self.extract_promise_constructor(type_annotation);

        // Build the awaiter call
        let awaiter_call = IRNode::AwaiterCall {
            this_arg: Box::new(IRNode::This { captured: false }),
            generator_body: Box::new(generator_body),
            hoisted_var_groups,
            promise_constructor,
        };

        // Build the function declaration/expression wrapper
        let ir_params: Vec<IRParam> = params.iter().map(|p| IRParam::new(p.clone())).collect();

        if let Some(func_name) = name {
            let mut body = hoisted_decls;
            self.emit_arguments_capture_decl(&mut body);
            body.push(awaiter_call);
            IRNode::FunctionDecl {
                name: func_name.into(),
                parameters: ir_params,
                body,
                body_source_range: None,
                leading_comment: None,
            }
        } else {
            let mut body = hoisted_decls;
            self.emit_arguments_capture_decl(&mut body);
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

    pub fn transform_generator_function(&mut self, func_idx: NodeIndex) -> IRNode {
        self.state.reset();
        self.reset_loop_exit_placeholders();
        self.generator_mode = true;
        self.helpers_needed.generator = true;
        let Some(node) = self.arena.get(func_idx) else {
            self.generator_mode = false;
            return IRNode::Undefined;
        };
        let (name, params, param_binding_names, body_idx) = if node.kind
            == syntax_kind_ext::FUNCTION_DECLARATION
            || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
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
                let mut param_binding_names = Vec::new();
                self.collect_parameter_binding_names(&func.parameters, &mut param_binding_names);
                (name, params, param_binding_names, func.body)
            } else {
                self.generator_mode = false;
                return IRNode::Undefined;
            }
        } else {
            self.generator_mode = false;
            return IRNode::Undefined;
        };
        let has_yield = self.body_contains_await(body_idx);
        self.state.has_await = has_yield;
        self.state.captures_arguments =
            tsz_parser::syntax::transform_utils::contains_arguments_reference(self.arena, body_idx);
        if self.state.captures_arguments {
            self.state.arguments_capture_name =
                self.fresh_arguments_capture_name(body_idx, &param_binding_names);
        }
        let mut generator_body = self.build_generator_body(body_idx, has_yield, &[]);
        let hoisted_var_groups = Self::extract_and_remove_var_decl_groups(&mut generator_body);
        let ir_params: Vec<IRParam> = params.iter().map(|p| IRParam::new(p.clone())).collect();
        let mut body = Vec::new();
        for group in hoisted_var_groups {
            let declarations = group
                .into_iter()
                .map(|name| IRNode::VarDecl {
                    name: name.into(),
                    initializer: None,
                })
                .collect();
            body.push(IRNode::VarDeclList(declarations));
        }
        if self.state.captures_arguments {
            body.push(IRNode::VarDecl {
                name: self.state.arguments_capture_name.clone().into(),
                initializer: Some(Box::new(IRNode::Raw("arguments".to_string().into()))),
            });
        }
        body.push(generator_body);
        self.generator_mode = false;
        if let Some(func_name) = name {
            IRNode::FunctionDecl {
                name: func_name.into(),
                parameters: ir_params,
                body,
                body_source_range: None,
                leading_comment: None,
            }
        } else {
            IRNode::FunctionExpr {
                name: None,
                parameters: ir_params,
                body,
                is_expression_body: false,
                body_source_range: None,
            }
        }
    }

    pub fn transform_async_generator_inner_function(
        &mut self,
        name: Option<String>,
        params: &[NodeIndex],
        body_idx: NodeIndex,
        include_params: bool,
    ) -> IRNode {
        self.state.reset();
        self.reset_loop_exit_placeholders();
        self.generator_mode = true;
        self.async_generator_mode = true;
        self.helpers_needed.await_helper = true;
        self.helpers_needed.async_generator = true;
        self.helpers_needed.generator = true;

        let mut param_binding_names = Vec::new();
        for &param_idx in params {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            self.collect_binding_name(param.name, &mut param_binding_names);
        }

        let has_yield = self.body_contains_await(body_idx);
        self.state.has_await = has_yield;
        self.state.captures_arguments =
            tsz_parser::syntax::transform_utils::contains_arguments_reference(self.arena, body_idx);
        if self.state.captures_arguments {
            self.state.arguments_capture_name =
                self.fresh_arguments_capture_name(body_idx, &param_binding_names);
        }

        let mut generator_body = self.build_generator_body(body_idx, has_yield, &[]);
        let hoisted_var_groups = Self::extract_and_remove_var_decl_groups(&mut generator_body);
        let mut body = Vec::new();
        for group in hoisted_var_groups {
            let declarations = group
                .into_iter()
                .map(|name| IRNode::VarDecl {
                    name: name.into(),
                    initializer: None,
                })
                .collect();
            body.push(IRNode::VarDeclList(declarations));
        }
        if self.state.captures_arguments {
            body.push(IRNode::VarDecl {
                name: self.state.arguments_capture_name.clone().into(),
                initializer: Some(Box::new(IRNode::Raw("arguments".to_string().into()))),
            });
        }
        body.push(generator_body);

        self.generator_mode = false;
        self.async_generator_mode = false;

        let ir_params = if include_params {
            params
                .iter()
                .filter_map(|&param_idx| {
                    let param_node = self.arena.get(param_idx)?;
                    let param = self.arena.get_parameter(param_node)?;
                    Some(IRParam::new(
                        crate::transforms::emit_utils::identifier_text_or_empty(
                            self.arena, param.name,
                        ),
                    ))
                })
                .collect()
        } else {
            Vec::new()
        };

        IRNode::FunctionExpr {
            name: name.map(Into::into),
            parameters: ir_params,
            body,
            is_expression_body: false,
            body_source_range: None,
        }
    }

    /// Extract a custom promise constructor expression from a function's return type annotation.
    fn extract_promise_constructor(&self, type_annotation: NodeIndex) -> Option<String> {
        let type_node = self.arena.get(type_annotation)?;
        if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }
        let type_ref = self.arena.get_type_ref(type_node)?;
        let type_name_node = self.arena.get(type_ref.type_name)?;
        if type_name_node.kind == syntax_kind_ext::QUALIFIED_NAME {
            Some(self.qualified_name_to_expression(type_ref.type_name))
        } else {
            None
        }
    }

    /// Convert a type name node (identifier or qualified name) to a JS expression string.
    fn qualified_name_to_expression(&self, idx: NodeIndex) -> String {
        let Some(node) = self.arena.get(idx) else {
            return String::new();
        };
        if node.kind == syntax_kind_ext::QUALIFIED_NAME
            && let Some(qn) = self.arena.get_qualified_name(node)
        {
            let left = self.qualified_name_to_expression(qn.left);
            let right =
                crate::transforms::emit_utils::identifier_text_or_empty(self.arena, qn.right);
            return format!("{left}.{right}");
        }
        crate::transforms::emit_utils::identifier_text_or_empty(self.arena, idx)
    }

    /// Transform just the generator body (for use by the wrapper)
    pub fn transform_generator_body(&mut self, body_idx: NodeIndex, has_await: bool) -> IRNode {
        self.state.reset();
        self.reset_loop_exit_placeholders();
        self.state.has_await = has_await;
        self.helpers_needed.generator = true;

        // Check if body references `arguments` — if so, rewrite to `arguments_1`
        // (the caller is responsible for emitting `var arguments_1 = arguments;`)
        self.state.captures_arguments =
            tsz_parser::syntax::transform_utils::contains_arguments_reference(self.arena, body_idx);
        if self.state.captures_arguments && self.state.arguments_capture_name.is_empty() {
            self.state.arguments_capture_name = self.fresh_arguments_capture_name(body_idx, &[]);
        }

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
                        comment: Some("return".to_string().into()),
                    },
                ))));
            }
            cases.push(IRGeneratorCase {
                label: current_label,
                statements: current_statements,
            });
        } else if !cases.is_empty() {
            cases.push(IRGeneratorCase {
                label: current_label,
                statements: vec![IRNode::ReturnStatement(Some(Box::new(
                    IRNode::GeneratorOp {
                        opcode: opcodes::RETURN,
                        value: None,
                        comment: Some("return".to_string().into()),
                    },
                )))],
            });
        } else if cases.is_empty() {
            // Empty async body - still need a return case
            cases.push(IRGeneratorCase {
                label: 0,
                statements: vec![IRNode::ReturnStatement(Some(Box::new(
                    IRNode::GeneratorOp {
                        opcode: opcodes::RETURN,
                        value: None,
                        comment: Some("return".to_string().into()),
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
                    if let Some(stmt_node) = self.arena.get(stmt_idx) {
                        let actual_start = super::emit_utils::skip_trivia_forward(
                            self.source_text,
                            stmt_node.pos,
                            stmt_node.end,
                        );
                        if let Some(comment) = self.extract_preceding_line_comment(actual_start) {
                            current_statements.push(IRNode::Raw(comment.into()));
                        }
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
        if node.kind == self.suspension_kind() {
            // return await/yield expr -> yield, then return _a.sent()
            self.process_await_expression(idx, cases, current_statements, current_label);
            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::RETURN,
                    value: Some(Box::new(IRNode::GeneratorSent)),
                    comment: Some("return".to_string().into()),
                },
            ))));
        } else if self.contains_await_recursive(idx) {
            let value = if let Some(lowered_call) = self.lower_call_callee_before_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                lowered_call
            } else {
                self.emit_nested_suspension(idx, cases, current_statements, current_label);
                self.expression_to_ir(idx)
            };
            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::RETURN,
                    value: Some(Box::new(value)),
                    comment: Some("return".to_string().into()),
                },
            ))));
        } else {
            // Non-await expression body: return the expression directly
            let value = self.expression_to_ir(idx);
            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::RETURN,
                    value: Some(Box::new(value)),
                    comment: Some("return".to_string().into()),
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
                                comment: Some("return".to_string().into()),
                            },
                        ))));
                    } else if self.is_suspension_expression(ret.expression) {
                        // return await/yield expr; -> yield, then return _a.sent()
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
                                comment: Some("return".to_string().into()),
                            },
                        ))));
                    } else if self.contains_await_recursive(ret.expression) {
                        let value = if let Some(lowered_call) = self
                            .lower_call_callee_before_suspension(
                                ret.expression,
                                cases,
                                current_statements,
                                current_label,
                            ) {
                            lowered_call
                        } else {
                            self.emit_nested_suspension(
                                ret.expression,
                                cases,
                                current_statements,
                                current_label,
                            );
                            self.expression_to_ir(ret.expression)
                        };
                        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                            IRNode::GeneratorOp {
                                opcode: opcodes::RETURN,
                                value: Some(Box::new(value)),
                                comment: Some("return".to_string().into()),
                            },
                        ))));
                    } else {
                        let value = self.expression_to_ir(ret.expression);
                        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                            IRNode::GeneratorOp {
                                opcode: opcodes::RETURN,
                                value: Some(Box::new(value)),
                                comment: Some("return".to_string().into()),
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

            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                if self.lower_class_extends_before_suspension(
                    idx,
                    cases,
                    current_statements,
                    current_label,
                ) {
                    return;
                }
                current_statements.push(self.statement_to_ir(idx));
            }

            k if k == syntax_kind_ext::IF_STATEMENT => {
                self.process_if_statement_in_async(idx, cases, current_statements, current_label);
            }

            k if k == syntax_kind_ext::WHILE_STATEMENT => {
                self.process_while_statement_in_async(
                    idx,
                    cases,
                    current_statements,
                    current_label,
                );
            }

            k if k == syntax_kind_ext::FOR_STATEMENT => {
                if !self.process_captured_for_statement_in_async(
                    idx,
                    cases,
                    current_statements,
                    current_label,
                ) {
                    current_statements.push(self.statement_to_ir(idx));
                }
            }

            k if k == syntax_kind_ext::THROW_STATEMENT => {
                if let Some(throw_data) = self.arena.get_return_statement(node) {
                    if self.contains_await_recursive(throw_data.expression) {
                        // throw await expr; -> yield expr, then throw _a.sent()
                        if self.is_suspension_expression(throw_data.expression) {
                            self.process_await_expression(
                                throw_data.expression,
                                cases,
                                current_statements,
                                current_label,
                            );
                            current_statements
                                .push(IRNode::ThrowStatement(Box::new(IRNode::GeneratorSent)));
                        } else {
                            self.emit_nested_suspension(
                                throw_data.expression,
                                cases,
                                current_statements,
                                current_label,
                            );
                            let expr = self.expression_to_ir(throw_data.expression);
                            current_statements.push(IRNode::ThrowStatement(Box::new(expr)));
                        }
                    } else {
                        let expr = self.expression_to_ir(throw_data.expression);
                        current_statements.push(IRNode::ThrowStatement(Box::new(expr)));
                    }
                }
            }

            k if k == syntax_kind_ext::TRY_STATEMENT => {
                self.process_try_statement_in_async(idx, cases, current_statements, current_label);
            }

            k if k == syntax_kind_ext::LABELED_STATEMENT => {
                self.process_labeled_statement_in_async(
                    idx,
                    cases,
                    current_statements,
                    current_label,
                );
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

        if self.lower_destructuring_assignment_expression(idx, current_statements) {
            return;
        }

        // Check for await expression
        if node.kind == self.suspension_kind() {
            self.process_await_expression(idx, cases, current_statements, current_label);
            // Add _a.sent() to consume the result
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::GeneratorSent)));
            return;
        }

        // Check for nested await inside the expression
        if self.contains_await_recursive(idx) {
            if self.lower_assignment_target_before_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                return;
            }
            if self.async_generator_mode
                && (node.kind == syntax_kind_ext::YIELD_EXPRESSION
                    || self.node_text_contains_yield(idx))
            {
                self.emit_nested_suspension(idx, cases, current_statements, current_label);
                self.push_generator_yield(
                    opcodes::YIELD,
                    IRNode::GeneratorSent,
                    "yield",
                    cases,
                    current_statements,
                    current_label,
                );
                current_statements
                    .push(IRNode::ExpressionStatement(Box::new(IRNode::GeneratorSent)));
                return;
            }
            self.emit_nested_suspension(idx, cases, current_statements, current_label);
            let ir = self.expression_to_ir(idx);
            current_statements.push(IRNode::ExpressionStatement(Box::new(ir)));
            return;
        }

        // For other expressions, convert to IR and add as expression statement
        let ir = self.expression_to_ir(idx);
        current_statements.push(IRNode::ExpressionStatement(Box::new(ir)));
    }

    fn lower_destructuring_assignment_expression(
        &self,
        idx: NodeIndex,
        current_statements: &mut Vec<IRNode>,
    ) -> bool {
        let target_idx = self.unwrap_parenthesized_expression(idx);
        let Some(node) = self.arena.get(target_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }
        let Some(bin) = self.arena.get_binary_expr(node) else {
            return false;
        };
        if self.get_operator_text(bin.operator_token) != "=" {
            return false;
        }
        let Some(left_node) = self.arena.get(bin.left) else {
            return false;
        };
        if left_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        let Some(pattern) = self.arena.get_literal_expr(left_node) else {
            return false;
        };
        if pattern.elements.nodes.is_empty() {
            return false;
        }

        let source = self.expression_to_ir(bin.right);
        for &elem_idx in &pattern.elements.nodes {
            let Some(assignment) = self.destructuring_object_assignment(elem_idx, source.clone())
            else {
                return false;
            };
            current_statements.push(IRNode::ExpressionStatement(Box::new(
                IRNode::Parenthesized(Box::new(assignment)),
            )));
        }
        true
    }

    fn unwrap_parenthesized_expression(&self, mut idx: NodeIndex) -> NodeIndex {
        while let Some(node) = self.arena.get(idx)
            && node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.arena.get_parenthesized(node)
        {
            idx = paren.expression;
        }
        idx
    }

    fn destructuring_object_assignment(
        &self,
        elem_idx: NodeIndex,
        source: IRNode,
    ) -> Option<IRNode> {
        let elem_node = self.arena.get(elem_idx)?;
        match elem_node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_property_assignment(elem_node)?;
                let target = self.expression_to_ir(prop.initializer);
                let value = self.destructuring_object_property_value(source, prop.name)?;
                Some(IRNode::assign(target, value))
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_shorthand_property(elem_node)?;
                let name =
                    crate::transforms::emit_utils::identifier_text_or_empty(self.arena, prop.name);
                let target = IRNode::id(name.clone());
                let value = IRNode::prop(source, name);
                Some(IRNode::assign(target, value))
            }
            _ => None,
        }
    }

    fn destructuring_object_property_value(
        &self,
        source: IRNode,
        name_idx: NodeIndex,
    ) -> Option<IRNode> {
        let name_node = self.arena.get(name_idx)?;
        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            let computed = self.arena.get_computed_property(name_node)?;
            return Some(IRNode::elem(
                source,
                self.expression_to_ir(computed.expression),
            ));
        }
        if name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            let name =
                crate::transforms::emit_utils::identifier_text_or_empty(self.arena, name_idx);
            return Some(IRNode::prop(source, name));
        }
        if name_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16 {
            let lit = self.arena.get_literal(name_node)?;
            return Some(IRNode::elem(source, IRNode::string(lit.text.clone())));
        }
        if name_node.kind == tsz_scanner::SyntaxKind::NumericLiteral as u16 {
            let lit = self.arena.get_literal(name_node)?;
            return Some(IRNode::elem(source, IRNode::number(lit.text.clone())));
        }
        None
    }

    fn emit_nested_suspension(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        if let Some(await_idx) = self.find_suspension_expression(idx) {
            self.process_await_expression(await_idx, cases, current_statements, current_label);
        }
    }

    fn find_suspension_expression(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.arena.get(idx)?;
        if node.kind == self.suspension_kind()
            || (self.async_generator_mode && node.kind == syntax_kind_ext::AWAIT_EXPRESSION)
        {
            return Some(idx);
        }
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            || node.is_function_expression_or_arrow()
        {
            return None;
        }
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin) = self.arena.get_binary_expr(node)
        {
            if let Some(found) = self.find_suspension_expression(bin.left) {
                return Some(found);
            }
            return self.find_suspension_expression(bin.right);
        }
        if node.kind == syntax_kind_ext::CALL_EXPRESSION
            && let Some(call) = self.arena.get_call_expr(node)
        {
            if let Some(found) = self.find_suspension_expression(call.expression) {
                return Some(found);
            }
            if let Some(args) = &call.arguments {
                for &arg_idx in &args.nodes {
                    if let Some(found) = self.find_suspension_expression(arg_idx) {
                        return Some(found);
                    }
                }
            }
        }
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.arena.get_access_expr(node)
        {
            return self.find_suspension_expression(access.expression);
        }
        if node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            && let Some(access) = self.arena.get_access_expr(node)
        {
            if let Some(found) = self.find_suspension_expression(access.expression) {
                return Some(found);
            }
            return self.find_suspension_expression(access.name_or_argument);
        }
        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.arena.get_parenthesized(node)
        {
            return self.find_suspension_expression(paren.expression);
        }
        // Type-only wrappers: `(await foo()) as T`, `<T>await foo()`,
        // `await foo() satisfies T`, `(await foo())!`. These are stripped
        // by `expression_to_ir`, so the analysis must look through them
        // too — otherwise we treat `var x = (await foo()) as T;` as
        // "no await" and emit `_a.sent()` without a preceding yield.
        if (node.kind == syntax_kind_ext::TYPE_ASSERTION
            || node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION)
            && let Some(assertion) = self.arena.get_type_assertion(node)
        {
            return self.find_suspension_expression(assertion.expression);
        }
        if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION
            && let Some(unary) = self.arena.get_unary_expr_ex(node)
        {
            return self.find_suspension_expression(unary.expression);
        }
        if node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
            && let Some(cond) = self.arena.get_conditional_expr(node)
        {
            if let Some(found) = self.find_suspension_expression(cond.condition) {
                return Some(found);
            }
            if let Some(found) = self.find_suspension_expression(cond.when_true) {
                return Some(found);
            }
            return self.find_suspension_expression(cond.when_false);
        }
        if node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            && let Some(unary) = self.arena.get_unary_expr(node)
        {
            return self.find_suspension_expression(unary.operand);
        }
        if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && let Some(computed) = self.arena.get_computed_property(node)
        {
            return self.find_suspension_expression(computed.expression);
        }
        if (node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            || node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
            && let Some(literal) = self.arena.get_literal_expr(node)
        {
            for &elem_idx in &literal.elements.nodes {
                let Some(elem_node) = self.arena.get(elem_idx) else {
                    continue;
                };

                if elem_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
                    && let Some(prop) = self.arena.get_property_assignment(elem_node)
                {
                    if let Some(found) = self.find_suspension_expression(prop.name) {
                        return Some(found);
                    }
                    if let Some(found) = self.find_suspension_expression(prop.initializer) {
                        return Some(found);
                    }
                } else if let Some(found) = self.find_suspension_expression(elem_idx) {
                    return Some(found);
                }
            }
        }
        None
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

        // await/yield uses UnaryExprDataEx
        if let Some(await_expr) = self.arena.get_unary_expr_ex(node) {
            if self.async_generator_mode && node.kind == syntax_kind_ext::YIELD_EXPRESSION {
                self.process_async_generator_yield_expression(
                    await_expr,
                    cases,
                    current_statements,
                    current_label,
                );
                return;
            }

            // Get the awaited expression
            let operand = if await_expr.expression.is_none() {
                IRNode::Raw("".to_string().into())
            } else if self.generator_mode && node.kind == syntax_kind_ext::YIELD_EXPRESSION {
                self.generator_yield_operand_to_ir(await_expr.expression)
            } else {
                let operand = self.expression_to_ir(await_expr.expression);
                if self.async_generator_mode && node.kind == syntax_kind_ext::AWAIT_EXPRESSION {
                    IRNode::CallExpr {
                        callee: Box::new(IRNode::RuntimeHelper("__await".into())),
                        arguments: vec![operand],
                    }
                } else {
                    operand
                }
            };

            // Emit: return [4 /*yield*/, operand];
            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::YIELD,
                    value: Some(Box::new(operand)),
                    comment: Some("yield".to_string().into()),
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

    fn process_async_generator_yield_expression(
        &mut self,
        yield_expr: &tsz_parser::parser::node::UnaryExprDataEx,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        if yield_expr.asterisk_token {
            let delegated = IRNode::CallExpr {
                callee: Box::new(IRNode::RuntimeHelper("__values".into())),
                arguments: vec![IRNode::CallExpr {
                    callee: Box::new(IRNode::RuntimeHelper("__asyncDelegator".into())),
                    arguments: vec![IRNode::CallExpr {
                        callee: Box::new(IRNode::RuntimeHelper("__asyncValues".into())),
                        arguments: vec![self.expression_to_ir(yield_expr.expression)],
                    }],
                }],
            };
            self.push_generator_yield(
                opcodes::YIELD_STAR,
                delegated,
                "yield*",
                cases,
                current_statements,
                current_label,
            );

            let awaited_delegated_value = IRNode::CallExpr {
                callee: Box::new(IRNode::PropertyAccess {
                    object: Box::new(IRNode::RuntimeHelper("__await".into())),
                    property: "apply".into(),
                }),
                arguments: vec![
                    IRNode::Undefined,
                    IRNode::ArrayLiteral(vec![IRNode::GeneratorSent]),
                ],
            };
            self.push_generator_yield(
                opcodes::YIELD,
                awaited_delegated_value,
                "yield",
                cases,
                current_statements,
                current_label,
            );
            return;
        }

        let operand = if self
            .arena
            .get(yield_expr.expression)
            .is_some_and(|n| n.kind == syntax_kind_ext::AWAIT_EXPRESSION)
        {
            let awaited = self
                .arena
                .get(yield_expr.expression)
                .and_then(|n| self.arena.get_unary_expr_ex(n))
                .map_or(IRNode::Undefined, |await_expr| {
                    self.wrap_async_generator_await(await_expr.expression)
                });
            self.push_generator_yield(
                opcodes::YIELD,
                awaited,
                "yield",
                cases,
                current_statements,
                current_label,
            );
            IRNode::GeneratorSent
        } else {
            self.wrap_async_generator_await(yield_expr.expression)
        };

        self.push_generator_yield(
            opcodes::YIELD,
            operand,
            "yield",
            cases,
            current_statements,
            current_label,
        );
        self.push_generator_yield(
            opcodes::YIELD,
            IRNode::GeneratorSent,
            "yield",
            cases,
            current_statements,
            current_label,
        );
    }

    fn push_generator_yield(
        &mut self,
        opcode: u32,
        value: IRNode,
        comment: &str,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
            IRNode::GeneratorOp {
                opcode,
                value: Some(Box::new(value)),
                comment: Some(comment.to_string().into()),
            },
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });
        *current_label = self.state.next_label();
    }

    fn wrap_async_generator_await(&self, expression: NodeIndex) -> IRNode {
        IRNode::CallExpr {
            callee: Box::new(IRNode::RuntimeHelper("__await".into())),
            arguments: vec![self.expression_to_ir(expression)],
        }
    }

    fn node_text_contains_yield(&self, idx: NodeIndex) -> bool {
        self.node_text_contains(idx, "yield")
    }

    fn node_text_contains(&self, idx: NodeIndex, needle: &str) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        let start = (node.pos as usize).min(text.len());
        let end = (node.end as usize).min(text.len());
        start < end && text[start..end].contains(needle)
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
            if decl.initializer.is_some() && self.is_suspension_expression(decl.initializer) {
                // var x = await foo(); -> first declare var x, then yield foo(), then x = _a.sent()
                // We need to declare the variable first to avoid ReferenceError in strict mode
                current_statements.push(IRNode::VarDecl {
                    name: name.clone().into(),
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
                        left: Box::new(IRNode::Identifier(name.into())),
                        operator: "=".to_string().into(),
                        right: Box::new(IRNode::GeneratorSent),
                    },
                )));
            } else if decl.initializer.is_some() && self.contains_await_recursive(decl.initializer)
            {
                // Initializer contains await but is not a direct await expression
                // (e.g., var x = (await foo()) + 1;)
                // Declare variable first, then process
                current_statements.push(IRNode::VarDecl {
                    name: name.clone().into(),
                    initializer: None,
                });

                if let Some((temp, initial_obj, lowered_init)) =
                    self.lower_object_literal_es5_after_computed_suspension(decl.initializer)
                {
                    current_statements.push(IRNode::HoistedVarGroupBreak);
                    current_statements.push(IRNode::VarDecl {
                        name: temp.clone().into(),
                        initializer: None,
                    });
                    current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                        IRNode::id(temp),
                        initial_obj,
                    ))));
                    self.emit_nested_suspension(
                        decl.initializer,
                        cases,
                        current_statements,
                        current_label,
                    );
                    current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                        IRNode::Identifier(name.into()),
                        lowered_init,
                    ))));
                    return;
                }

                // Emit the yield for the nested await
                if let Some(lowered_init) = self.lower_call_callee_before_suspension(
                    decl.initializer,
                    cases,
                    current_statements,
                    current_label,
                ) {
                    current_statements.push(IRNode::ExpressionStatement(Box::new(
                        IRNode::BinaryExpr {
                            left: Box::new(IRNode::Identifier(name.into())),
                            operator: "=".to_string().into(),
                            right: Box::new(lowered_init),
                        },
                    )));
                    return;
                }

                self.emit_nested_suspension(
                    decl.initializer,
                    cases,
                    current_statements,
                    current_label,
                );
                let init = self.expression_to_ir(decl.initializer);
                current_statements.push(IRNode::ExpressionStatement(Box::new(
                    IRNode::BinaryExpr {
                        left: Box::new(IRNode::Identifier(name.into())),
                        operator: "=".to_string().into(),
                        right: Box::new(init),
                    },
                )));
            } else {
                // No await in initializer - emit as normal
                if let Some((temp, lowered_init)) =
                    self.lower_object_literal_es5_with_computed_properties(decl.initializer)
                {
                    current_statements.push(IRNode::VarDecl {
                        name: name.clone().into(),
                        initializer: None,
                    });
                    current_statements.push(IRNode::VarDecl {
                        name: temp.into(),
                        initializer: None,
                    });
                    current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                        IRNode::Identifier(name.into()),
                        lowered_init,
                    ))));
                    return;
                }

                let init = if decl.initializer.is_none() {
                    None
                } else {
                    Some(Box::new(self.expression_to_ir(decl.initializer)))
                };

                current_statements.push(IRNode::VarDecl {
                    name: name.into(),
                    initializer: init,
                });
            }
        }
    }

    fn lower_assignment_target_before_suspension(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }
        let Some(bin) = self.arena.get_binary_expr(node) else {
            return false;
        };
        if self.get_operator_text(bin.operator_token) != "=" {
            return false;
        }
        if !self.contains_await_recursive(bin.right) || self.contains_await_recursive(bin.left) {
            return false;
        }
        let Some(left_node) = self.arena.get(bin.left) else {
            return false;
        };

        let Some((target, object)) = self.suspended_assignment_target(left_node) else {
            return false;
        };
        let temp = self.generate_hoisted_temp();
        current_statements.push(IRNode::VarDecl {
            name: temp.clone().into(),
            initializer: None,
        });
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(temp.clone()),
            object,
        ))));

        self.emit_nested_suspension(idx, cases, current_statements, current_label);

        let lowered_target = match target {
            SuspendedAssignmentTarget::Property(property) => {
                IRNode::prop(IRNode::id(temp), property)
            }
            SuspendedAssignmentTarget::Element(index) => IRNode::elem(IRNode::id(temp), *index),
        };
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            lowered_target,
            self.expression_to_ir(bin.right),
        ))));
        true
    }

    fn suspended_assignment_target(
        &self,
        left_node: &tsz_parser::parser::node::Node,
    ) -> Option<(SuspendedAssignmentTarget, IRNode)> {
        if left_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(left_node)?;
            let object = self.expression_to_ir(access.expression);
            let property = crate::transforms::emit_utils::identifier_text_or_empty(
                self.arena,
                access.name_or_argument,
            );
            return Some((SuspendedAssignmentTarget::Property(property), object));
        }

        if left_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(left_node)?;
            let object = self.expression_to_ir(access.expression);
            let index = self.expression_to_ir(access.name_or_argument);
            return Some((SuspendedAssignmentTarget::Element(Box::new(index)), object));
        }

        None
    }

    fn lower_call_callee_before_suspension(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> Option<IRNode> {
        let node = self.arena.get(idx)?;
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.arena.get_call_expr(node)?;
        if self.contains_await_recursive(call.expression) {
            return None;
        }
        let args = call.arguments.as_ref()?;
        let suspension_arg_index = args
            .nodes
            .iter()
            .position(|&arg| self.contains_await_recursive(arg))?;

        let (callee_temp, this_arg) =
            self.capture_call_callee_before_suspension(call.expression, current_statements)?;
        let arg_array = self.lower_suspended_call_arguments(
            &args.nodes,
            suspension_arg_index,
            current_statements,
        );

        self.emit_nested_suspension(idx, cases, current_statements, current_label);

        Some(IRNode::CallExpr {
            callee: Box::new(IRNode::prop(IRNode::id(callee_temp), "apply")),
            arguments: vec![this_arg, arg_array],
        })
    }

    fn capture_call_callee_before_suspension(
        &self,
        callee: NodeIndex,
        current_statements: &mut Vec<IRNode>,
    ) -> Option<(String, IRNode)> {
        let callee_node = self.arena.get(callee)?;

        if callee_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            let callee_temp = self.generate_hoisted_temp();
            current_statements.push(IRNode::VarDecl {
                name: callee_temp.clone().into(),
                initializer: None,
            });
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(callee_temp.clone()),
                self.expression_to_ir(callee),
            ))));
            return Some((callee_temp, IRNode::Undefined));
        }

        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }

        let access = self.arena.get_access_expr(callee_node)?;
        let this_temp = self.generate_hoisted_temp();
        let callee_temp = self.generate_hoisted_temp();
        current_statements.push(IRNode::VarDecl {
            name: this_temp.clone().into(),
            initializer: None,
        });
        current_statements.push(IRNode::VarDecl {
            name: callee_temp.clone().into(),
            initializer: None,
        });
        let property = crate::transforms::emit_utils::identifier_text_or_empty(
            self.arena,
            access.name_or_argument,
        );
        let captured_receiver = IRNode::Parenthesized(Box::new(IRNode::assign(
            IRNode::id(this_temp.clone()),
            self.expression_to_ir(access.expression),
        )));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(callee_temp.clone()),
            IRNode::prop(captured_receiver, property),
        ))));

        Some((callee_temp, IRNode::id(this_temp)))
    }

    fn lower_suspended_call_arguments(
        &self,
        args: &[NodeIndex],
        suspension_arg_index: usize,
        current_statements: &mut Vec<IRNode>,
    ) -> IRNode {
        if suspension_arg_index == 0 {
            let lowered_args = args.iter().map(|&arg| self.expression_to_ir(arg)).collect();
            return IRNode::ArrayLiteral(lowered_args);
        }

        let prefix_temp = self.generate_hoisted_temp();
        current_statements.push(IRNode::VarDecl {
            name: prefix_temp.clone().into(),
            initializer: None,
        });
        let prefix_args = args[..suspension_arg_index]
            .iter()
            .map(|&arg| self.expression_to_ir(arg))
            .collect();
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(prefix_temp.clone()),
            IRNode::ArrayLiteral(prefix_args),
        ))));

        let suffix_args = args[suspension_arg_index..]
            .iter()
            .map(|&arg| self.expression_to_ir(arg))
            .collect();
        IRNode::CallExpr {
            callee: Box::new(IRNode::prop(IRNode::id(prefix_temp), "concat")),
            arguments: vec![IRNode::ArrayLiteral(suffix_args)],
        }
    }

    fn lower_class_extends_before_suspension(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> bool {
        let Some((class_name, extends_expr, suspension_idx)) = self.class_extends_suspension(idx)
        else {
            return false;
        };
        let Some(factory_parts) = self.es5_class_factory(idx, &class_name) else {
            return false;
        };

        let factory_temp = self.generate_hoisted_temp();

        // Emit weakmap declarations alongside the other class-related vars.
        // These would otherwise be silently dropped by destructuring just the
        // factory body out of the ES5ClassIIFE IR node.
        for weakmap_name in &factory_parts.weakmap_decls {
            current_statements.push(IRNode::VarDecl {
                name: weakmap_name.clone().into(),
                initializer: None,
            });
        }

        current_statements.push(IRNode::VarDecl {
            name: class_name.clone().into(),
            initializer: None,
        });
        current_statements.push(IRNode::VarDecl {
            name: factory_temp.clone().into(),
            initializer: None,
        });
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(factory_temp.clone()),
            factory_parts.factory,
        ))));

        self.process_await_expression(suspension_idx, cases, current_statements, current_label);

        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(class_name),
            IRNode::ES5ClassApply {
                factory: Box::new(IRNode::id(factory_temp)),
                base_class: Box::new(self.extends_value_after_suspension(extends_expr)),
            },
        ))));

        // Emit weakmap initializers and deferred static blocks after the class
        // is assigned, mirroring the ordering used by IRPrinter for
        // ES5ClassIIFE (see `ir_printer.rs` ES5ClassIIFE arm: weakmap_inits
        // appended after the IIFE, then deferred_static_blocks).
        for weakmap_init in factory_parts.weakmap_inits {
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::Raw(
                weakmap_init.into(),
            ))));
        }
        for deferred in factory_parts.deferred_static_blocks {
            current_statements.push(deferred);
        }

        true
    }

    fn class_extends_suspension(
        &self,
        class_idx: NodeIndex,
    ) -> Option<(String, NodeIndex, NodeIndex)> {
        let node = self.arena.get(class_idx)?;
        let class_data = self.arena.get_class(node)?;
        let class_name =
            crate::transforms::emit_utils::identifier_text_or_empty(self.arena, class_data.name);
        if class_name.is_empty() {
            return None;
        }
        let extends_expr = crate::transforms::emit_utils::get_extends_expression_index(
            self.arena,
            &class_data.heritage_clauses,
        )?;
        let suspension_idx = self.find_suspension_expression(extends_expr)?;
        Some((class_name, extends_expr, suspension_idx))
    }

    fn es5_class_factory(
        &self,
        class_idx: NodeIndex,
        class_name: &str,
    ) -> Option<ES5ClassFactoryParts> {
        let mut class_transformer = ES5ClassTransformer::new(self.arena);
        let class_ir = class_transformer.transform_class_with_name(class_idx, Some(class_name))?;
        let IRNode::ES5ClassIIFE {
            body,
            super_param,
            weakmap_decls,
            computed_prop_temp_decls: _,
            computed_prop_temp_inits: _,
            weakmap_inits,
            deferred_static_blocks,
            ..
        } = class_ir
        else {
            return None;
        };
        Some(ES5ClassFactoryParts {
            factory: IRNode::FunctionExpr {
                name: None,
                parameters: vec![IRParam::new(
                    super_param.as_deref().unwrap_or("_super").to_string(),
                )],
                body,
                is_expression_body: false,
                body_source_range: None,
            },
            weakmap_decls,
            weakmap_inits,
            deferred_static_blocks,
        })
    }

    fn extends_value_after_suspension(&self, extends_expr: NodeIndex) -> IRNode {
        let stripped = self.strip_parenthesized_expression(extends_expr);
        if self.is_suspension_expression(stripped) {
            IRNode::GeneratorSent
        } else {
            self.expression_to_ir(extends_expr)
        }
    }

    fn strip_parenthesized_expression(&self, mut idx: NodeIndex) -> NodeIndex {
        loop {
            let Some(node) = self.arena.get(idx) else {
                return idx;
            };
            if node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                return idx;
            }
            let Some(paren) = self.arena.get_parenthesized(node) else {
                return idx;
            };
            idx = paren.expression;
        }
    }

    // =========================================================================
    // Control flow statement processing for async state machine
    // =========================================================================

    /// Process an if statement inside an async function body.
    ///
    /// When neither branch contains await, falls through to raw IR emission.
    /// When branches contain await, generates proper state machine labels.
    fn process_if_statement_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };
        let Some(if_stmt) = self.arena.get_if_statement(node) else {
            return;
        };

        let then_has_await = self.contains_await_recursive(if_stmt.then_statement);
        let else_has_await = if_stmt.else_statement.is_some()
            && self.contains_await_recursive(if_stmt.else_statement);

        if !then_has_await && !else_has_await {
            // No await in either branch -- emit as-is
            let ir = self.statement_to_ir(idx);
            current_statements.push(ir);
            return;
        }

        let has_else = if_stmt.else_statement.is_some()
            && self
                .arena
                .get(if_stmt.else_statement)
                .is_some_and(|n| n.kind != syntax_kind_ext::EMPTY_STATEMENT);

        // Reserve labels for else branch and end
        let else_label = self.state.next_label();
        let end_label = if has_else {
            self.state.next_label()
        } else {
            else_label
        };

        // Emit: if (!(condition)) return [3 /*break*/, else_label];
        let target_label = if has_else { else_label } else { end_label };
        let cond_ir = self.expression_to_ir(if_stmt.expression);
        current_statements.push(IRNode::IfBreak {
            condition: Box::new(IRNode::PrefixUnaryExpr {
                operator: "!".to_string().into(),
                operand: Box::new(cond_ir),
            }),
            target_label,
        });

        // Process then branch
        self.process_block_or_statement_in_async(
            if_stmt.then_statement,
            cases,
            current_statements,
            current_label,
        );

        if has_else {
            // Emit: return [3 /*break*/, end_label]; at end of then branch
            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::BREAK,
                    value: Some(Box::new(IRNode::NumericLiteral(
                        end_label.to_string().into(),
                    ))),
                    comment: Some("break".to_string().into()),
                },
            ))));

            // Flush current case and start else branch
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
            *current_label = else_label;

            // Process else branch
            self.process_block_or_statement_in_async(
                if_stmt.else_statement,
                cases,
                current_statements,
                current_label,
            );
        }

        // Flush current case and start end label
        if !current_statements.is_empty() {
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
        }
        *current_label = end_label;
    }

    /// Process a while statement inside an async function body.
    ///
    /// `await` in the body must be lifted into generator cases before the loop
    /// body is emitted. A raw `while` statement around `await` would otherwise
    /// leave invalid `await` syntax inside the ES5 generator callback.
    fn process_while_statement_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };
        let Some(loop_data) = self.arena.get_loop(node) else {
            return;
        };

        let condition_has_await = self.contains_await_recursive(loop_data.condition);
        let body_has_await = self.contains_await_recursive(loop_data.statement);

        if !body_has_await || condition_has_await {
            current_statements.push(self.statement_to_ir(idx));
            return;
        }

        let loop_label = *current_label;
        let exit_placeholder = self.next_loop_exit_placeholder();
        let condition = self.expression_to_ir(loop_data.condition);

        current_statements.push(IRNode::IfBreak {
            condition: Box::new(IRNode::PrefixUnaryExpr {
                operator: "!".to_string().into(),
                operand: Box::new(condition),
            }),
            target_label: exit_placeholder,
        });

        self.process_block_or_statement_in_async(
            loop_data.statement,
            cases,
            current_statements,
            current_label,
        );

        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
            IRNode::GeneratorOp {
                opcode: opcodes::BREAK,
                value: Some(Box::new(IRNode::NumericLiteral(
                    loop_label.to_string().into(),
                ))),
                comment: Some("break".to_string().into()),
            },
        ))));

        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        let exit_label = self.state.next_label();
        Self::patch_if_break_target(cases, exit_placeholder, exit_label);
        *current_label = exit_label;
    }

    fn process_captured_for_statement_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        let Some(loop_data) = self.arena.get_loop(node) else {
            return false;
        };
        if !self.loop_needs_async_capture(idx) {
            return false;
        }

        let Some((loop_var, init_text)) =
            self.simple_for_loop_var_initializer(loop_data.initializer)
        else {
            return false;
        };

        let loop_suffix = self.async_captured_for_loop_ordinal(idx);
        let loop_fn = format!("_loop_{loop_suffix}");
        let state_name = self.captured_for_loop_state_name(idx);
        let condition = self.ir_text(self.expression_to_ir(loop_data.condition));
        let incrementor = self.ir_text(self.expression_to_ir(loop_data.incrementor));
        let inner_body = self.captured_for_loop_inner_generator(loop_data.statement, &state_name);

        current_statements.push(IRNode::VarDecl {
            name: loop_fn.clone().into(),
            initializer: None,
        });
        current_statements.push(IRNode::Raw(
            format!(
                "{loop_fn} = function ({loop_var}) {{\n                        return __generator(this, function (_b) {{\n                            switch (_b.label) {{\n{inner_body}                            }}\n                        }});\n                    }};"
            )
            .into(),
        ));
        current_statements.push(IRNode::VarDecl {
            name: loop_var.clone().into(),
            initializer: None,
        });
        if let Some(state_name) = &state_name {
            current_statements.push(IRNode::VarDecl {
                name: state_name.clone().into(),
                initializer: None,
            });
        }
        current_statements.push(IRNode::Raw(format!("{loop_var} = {init_text};").into()));
        current_statements.push(IRNode::Raw(
            format!("_a.label = {};", self.state.label_counter).into(),
        ));

        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        let condition_label = self.state.next_label();
        *current_label = condition_label;
        let after_yield_label = self.state.next_label();
        let increment_label = self.state.next_label();
        let exit_label = self.state.next_label();

        current_statements.push(IRNode::Raw(
            format!("if (!({condition})) return [3 /*break*/, {exit_label}];").into(),
        ));
        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
            IRNode::GeneratorOp {
                opcode: opcodes::YIELD_STAR,
                value: Some(Box::new(IRNode::CallExpr {
                    callee: Box::new(IRNode::Identifier(loop_fn.into())),
                    arguments: vec![IRNode::Identifier(loop_var.into())],
                })),
                comment: Some("yield*".to_string().into()),
            },
        ))));
        cases.push(IRGeneratorCase {
            label: condition_label,
            statements: std::mem::take(current_statements),
        });

        if let Some(state_name) = &state_name {
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::BinaryExpr {
                left: Box::new(IRNode::Identifier(state_name.clone().into())),
                operator: "=".to_string().into(),
                right: Box::new(IRNode::GeneratorSent),
            })));
            if self.captured_for_loop_has_break(loop_data.statement) {
                current_statements.push(IRNode::Raw(format!(
                    "if ({state_name} === \"break\")\n                        return [3 /*break*/, {exit_label}];"
                ).into()));
            }
            if self.captured_for_loop_has_value_return(loop_data.statement) {
                current_statements.push(IRNode::Raw(format!(
                    "if (typeof {state_name} === \"object\")\n                        return [2 /*return*/, {state_name}.value];"
                ).into()));
            }
        } else {
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::GeneratorSent)));
        }
        current_statements.push(IRNode::Raw(format!("_a.label = {increment_label};").into()));
        cases.push(IRGeneratorCase {
            label: after_yield_label,
            statements: std::mem::take(current_statements),
        });

        current_statements.push(IRNode::Raw(format!("{incrementor};").into()));
        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
            IRNode::GeneratorOp {
                opcode: opcodes::BREAK,
                value: Some(Box::new(IRNode::NumericLiteral(
                    condition_label.to_string().into(),
                ))),
                comment: Some("break".to_string().into()),
            },
        ))));
        cases.push(IRGeneratorCase {
            label: increment_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = exit_label;
        true
    }

    fn loop_needs_async_capture(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        let Some(loop_data) = self.arena.get_loop(node) else {
            return false;
        };
        if !self.contains_await_recursive(loop_data.statement) {
            return false;
        }
        let loop_vars = crate::transforms::block_scoping_es5::collect_loop_vars(
            self.arena,
            loop_data.initializer,
        );
        if loop_vars.is_empty() {
            return false;
        }
        crate::transforms::block_scoping_es5::analyze_loop_capture(
            self.arena,
            loop_data.statement,
            &loop_vars,
        )
        .needs_capture
    }

    fn async_captured_for_loop_ordinal(&self, idx: NodeIndex) -> usize {
        let Some(current) = self.arena.get(idx) else {
            return 1;
        };
        self.arena
            .nodes
            .iter()
            .enumerate()
            .filter(|(i, node)| {
                node.pos <= current.pos
                    && node.kind == syntax_kind_ext::FOR_STATEMENT
                    && self.loop_needs_async_capture(NodeIndex(*i as u32))
            })
            .count()
    }

    fn captured_for_loop_state_name(&self, idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(idx)?;
        let loop_data = self.arena.get_loop(node)?;
        if !self.captured_for_loop_has_break(loop_data.statement)
            && !self.captured_for_loop_has_value_return(loop_data.statement)
        {
            return None;
        }
        let current = self.arena.get(idx)?;
        let ordinal = self
            .arena
            .nodes
            .iter()
            .enumerate()
            .filter(|(i, node)| {
                node.pos <= current.pos
                    && node.kind == syntax_kind_ext::FOR_STATEMENT
                    && self.loop_needs_async_capture(NodeIndex(*i as u32))
                    && self.arena.get_loop(node).is_some_and(|loop_data| {
                        self.captured_for_loop_has_break(loop_data.statement)
                            || self.captured_for_loop_has_value_return(loop_data.statement)
                    })
            })
            .count();
        Some(format!("state_{ordinal}"))
    }

    fn simple_for_loop_var_initializer(&self, initializer: NodeIndex) -> Option<(String, String)> {
        let init_node = self.arena.get(initializer)?;
        let var_list = self.arena.get_variable(init_node)?;
        let decl_idx = *var_list.declarations.nodes.first()?;
        let decl = self.arena.get_variable_declaration_at(decl_idx)?;
        Some((
            crate::transforms::emit_utils::identifier_text_or_empty(self.arena, decl.name),
            self.ir_text(self.expression_to_ir(decl.initializer)),
        ))
    }

    fn captured_for_loop_inner_generator(
        &mut self,
        body: NodeIndex,
        state_name: &Option<String>,
    ) -> String {
        let mut lines = Vec::new();
        lines.push("                                case 0: return [4 /*yield*/, 1];".to_string());
        lines.push("                                case 1:".to_string());
        lines.push("                                    _b.sent();".to_string());

        if let Some(block_node) = self.arena.get(body)
            && let Some(block) = self.arena.get_block(block_node)
        {
            for &stmt_idx in &block.statements.nodes {
                let Some(stmt_node) = self.arena.get(stmt_idx) else {
                    continue;
                };
                if stmt_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT {
                    if let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node)
                        && self.is_suspension_expression(expr_stmt.expression)
                    {
                        continue;
                    }
                    lines.push(format!(
                        "                                    {}",
                        self.ir_text(self.statement_to_ir(stmt_idx))
                    ));
                } else if stmt_node.kind == syntax_kind_ext::BREAK_STATEMENT {
                    lines.push(
                        "                                    return [2 /*return*/, \"break\"];"
                            .to_string(),
                    );
                } else if stmt_node.kind == syntax_kind_ext::CONTINUE_STATEMENT {
                    lines.push(
                        "                                    return [2 /*return*/, \"continue\"];"
                            .to_string(),
                    );
                } else if stmt_node.kind == syntax_kind_ext::RETURN_STATEMENT {
                    if let Some(ret) = self.arena.get_return_statement(stmt_node) {
                        let value = self.ir_text(self.expression_to_ir(ret.expression));
                        lines.push(format!(
                            "                                    return [2 /*return*/, {{ value: {value} }}];"
                        ));
                    }
                }
            }
        }

        if state_name.is_none() && !self.captured_for_loop_has_continue(body) {
            lines.push("                                    return [2 /*return*/];".to_string());
        }
        lines.join("\n") + "\n"
    }

    fn captured_for_loop_has_break(&self, body: NodeIndex) -> bool {
        self.block_contains_statement_kind(body, syntax_kind_ext::BREAK_STATEMENT)
    }

    fn captured_for_loop_has_continue(&self, body: NodeIndex) -> bool {
        self.block_contains_statement_kind(body, syntax_kind_ext::CONTINUE_STATEMENT)
    }

    fn captured_for_loop_has_value_return(&self, body: NodeIndex) -> bool {
        self.block_contains_statement_kind(body, syntax_kind_ext::RETURN_STATEMENT)
    }

    fn block_contains_statement_kind(&self, body: NodeIndex, kind: u16) -> bool {
        let Some(block_node) = self.arena.get(body) else {
            return false;
        };
        let Some(block) = self.arena.get_block(block_node) else {
            return false;
        };
        block.statements.nodes.iter().any(|&stmt_idx| {
            self.arena
                .get(stmt_idx)
                .is_some_and(|stmt_node| stmt_node.kind == kind)
        })
    }

    fn ir_text(&self, ir: IRNode) -> String {
        crate::transforms::ir_printer::IRPrinter::emit_to_string(&ir)
    }

    fn patch_if_break_target(
        cases: &mut [IRGeneratorCase],
        placeholder_label: u32,
        target_label: u32,
    ) {
        for case in cases {
            for statement in &mut case.statements {
                Self::patch_if_break_target_in_node(statement, placeholder_label, target_label);
            }
        }
    }

    const fn patch_if_break_target_in_node(
        node: &mut IRNode,
        placeholder_label: u32,
        target_label: u32,
    ) {
        if let IRNode::IfBreak {
            target_label: candidate,
            ..
        } = node
            && *candidate == placeholder_label
        {
            *candidate = target_label;
        }
    }

    /// Process a try/catch/finally statement inside an async function body.
    ///
    /// When none of the blocks contain await, falls through to raw IR emission.
    /// When blocks contain await, generates proper state machine labels with
    /// try/catch/finally opcodes.
    fn process_try_statement_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };
        let Some(try_data) = self.arena.get_try(node) else {
            return;
        };

        let try_has_await = self.contains_await_recursive(try_data.try_block);
        let catch_has_await = self.contains_await_recursive(try_data.catch_clause);
        let finally_has_await = self.contains_await_recursive(try_data.finally_block);

        if !try_has_await && !catch_has_await && !finally_has_await {
            // No await in any block -- emit as-is
            let ir = self.statement_to_ir(idx);
            current_statements.push(ir);
            return;
        }

        let has_catch =
            try_data.catch_clause.is_some() && self.arena.get(try_data.catch_clause).is_some();
        let has_finally =
            try_data.finally_block.is_some() && self.arena.get(try_data.finally_block).is_some();

        // Reserve labels
        let catch_label = if has_catch {
            Some(self.state.next_label())
        } else {
            None
        };
        let finally_label = if has_finally {
            Some(self.state.next_label())
        } else {
            None
        };
        let end_label = self.state.next_label();

        // Build try-op instruction: _a.trys.push([currentLabel, catchLabel, finallyLabel, endLabel])
        let mut try_op_labels = vec![IRNode::NumericLiteral(current_label.to_string().into())];
        if let Some(cl) = catch_label {
            try_op_labels.push(IRNode::NumericLiteral(cl.to_string().into()));
        }
        if let Some(fl) = finally_label {
            if catch_label.is_none() {
                try_op_labels.push(IRNode::Undefined); // placeholder for missing catch
            }
            try_op_labels.push(IRNode::NumericLiteral(fl.to_string().into()));
        }
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::CallExpr {
            callee: Box::new(IRNode::PropertyAccess {
                object: Box::new(IRNode::PropertyAccess {
                    object: Box::new(IRNode::Identifier("_a".to_string().into())),
                    property: "trys".to_string().into(),
                }),
                property: "push".to_string().into(),
            }),
            arguments: vec![IRNode::ArrayLiteral(try_op_labels)],
        })));

        // Process try block
        self.process_block_or_statement_in_async(
            try_data.try_block,
            cases,
            current_statements,
            current_label,
        );

        // Break to finally or end
        let jump_target = finally_label.unwrap_or(end_label);
        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
            IRNode::GeneratorOp {
                opcode: opcodes::BREAK,
                value: Some(Box::new(IRNode::NumericLiteral(
                    jump_target.to_string().into(),
                ))),
                comment: Some("break".to_string().into()),
            },
        ))));

        // Catch block
        if let Some(cl) = catch_label {
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
            *current_label = cl;

            // Extract catch variable name
            if let Some(catch_node) = self.arena.get(try_data.catch_clause)
                && let Some(catch_data) = self.arena.get_catch_clause(catch_node)
            {
                // Declare catch variable: e_1 = _a.sent()
                if catch_data.variable_declaration.is_some() {
                    let catch_var_name =
                        self.get_catch_variable_name(catch_data.variable_declaration);
                    if !catch_var_name.is_empty() {
                        current_statements.push(IRNode::ExpressionStatement(Box::new(
                            IRNode::BinaryExpr {
                                left: Box::new(IRNode::Identifier(catch_var_name.into())),
                                operator: "=".to_string().into(),
                                right: Box::new(IRNode::ElementAccess {
                                    object: Box::new(IRNode::Identifier("_a".to_string().into())),
                                    index: Box::new(IRNode::NumericLiteral("1".to_string().into())),
                                }),
                            },
                        )));
                    }
                }

                // Process catch block body
                self.process_block_or_statement_in_async(
                    catch_data.block,
                    cases,
                    current_statements,
                    current_label,
                );
            }

            // Break to finally or end
            let jump_target = finally_label.unwrap_or(end_label);
            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::BREAK,
                    value: Some(Box::new(IRNode::NumericLiteral(
                        jump_target.to_string().into(),
                    ))),
                    comment: Some("break".to_string().into()),
                },
            ))));
        }

        // Finally block
        if let Some(fl) = finally_label {
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
            *current_label = fl;

            // Process finally block body
            self.process_block_or_statement_in_async(
                try_data.finally_block,
                cases,
                current_statements,
                current_label,
            );

            // End finally: return [7]
            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::END_FINALLY,
                    value: None,
                    comment: Some("endfinally".to_string().into()),
                },
            ))));
        }

        // Flush and start end label
        if !current_statements.is_empty() {
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
        }
        *current_label = end_label;
    }

    fn process_labeled_statement_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };
        let Some(labeled) = self.arena.get_labeled_statement(node) else {
            return;
        };

        if !self.contains_await_recursive(labeled.statement) {
            current_statements.push(self.statement_to_ir(idx));
            return;
        }

        let label =
            crate::transforms::emit_utils::identifier_text_or_empty(self.arena, labeled.label);

        let Some(statement_node) = self.arena.get(labeled.statement) else {
            return;
        };
        if statement_node.kind == syntax_kind_ext::BLOCK
            && let Some(block) = self.arena.get_block(statement_node)
        {
            for &stmt_idx in &block.statements.nodes {
                if self.is_break_to_label(stmt_idx, &label) {
                    let end_label = self.state.next_label();
                    current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                        IRNode::GeneratorOp {
                            opcode: opcodes::BREAK,
                            value: Some(Box::new(IRNode::NumericLiteral(
                                end_label.to_string().into(),
                            ))),
                            comment: Some("break".to_string().into()),
                        },
                    ))));
                    cases.push(IRGeneratorCase {
                        label: *current_label,
                        statements: std::mem::take(current_statements),
                    });
                    *current_label = end_label;
                    return;
                }

                self.process_async_statement(stmt_idx, cases, current_statements, current_label);
            }
        } else {
            self.process_async_statement(
                labeled.statement,
                cases,
                current_statements,
                current_label,
            );
        }
    }

    fn is_break_to_label(&self, stmt_idx: NodeIndex, label: &str) -> bool {
        let Some(node) = self.arena.get(stmt_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::BREAK_STATEMENT {
            return false;
        }
        let Some(jump) = self.arena.get_jump_data(node) else {
            return false;
        };
        crate::transforms::emit_utils::identifier_text_or_empty(self.arena, jump.label) == label
    }

    /// Get the catch variable name from a variable declaration index
    fn get_catch_variable_name(&self, var_decl_idx: NodeIndex) -> String {
        if let Some(var_node) = self.arena.get(var_decl_idx)
            && let Some(var_decl) = self.arena.get_variable_declaration(var_node)
        {
            crate::transforms::emit_utils::identifier_text_or_empty(self.arena, var_decl.name)
        } else {
            crate::transforms::emit_utils::identifier_text_or_empty(self.arena, var_decl_idx)
        }
    }

    /// Process either a block or single statement in async context.
    /// Used by if/else and try/catch to handle both `{ ... }` and single-statement branches.
    fn process_block_or_statement_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

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
        } else {
            self.process_async_statement(idx, cases, current_statements, current_label);
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
        if node.kind == self.suspension_kind()
            || (self.async_generator_mode && node.kind == syntax_kind_ext::AWAIT_EXPRESSION)
        {
            return true;
        }

        // Don't recurse into nested functions
        // This check must happen before recursing into any children
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            || node.is_function_expression_or_arrow()
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

        // Class method bodies are function-like scopes, but heritage clauses are
        // evaluated in the surrounding async function.
        if (node.kind == syntax_kind_ext::CLASS_DECLARATION
            || node.kind == syntax_kind_ext::CLASS_EXPRESSION)
            && let Some(class_data) = self.arena.get_class(node)
        {
            if let Some(extends_expr) = crate::transforms::emit_utils::get_extends_expression_index(
                self.arena,
                &class_data.heritage_clauses,
            ) && self.contains_await_recursive(extends_expr)
            {
                return true;
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

        // Type-only expression wrappers (TS-only syntax stripped by
        // `expression_to_ir`). Analysis must look through them too so
        // that `(await foo()) as T` is detected as containing an await.
        if (node.kind == syntax_kind_ext::TYPE_ASSERTION
            || node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION)
            && let Some(assertion) = self.arena.get_type_assertion(node)
        {
            return self.contains_await_recursive(assertion.expression);
        }
        if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION
            && let Some(unary) = self.arena.get_unary_expr_ex(node)
        {
            return self.contains_await_recursive(unary.expression);
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
        super::emit_utils::param_initializer_has_top_level_await(self.arena, param_idx)
    }

    fn first_await_default_param_name(
        &self,
        params: &tsz_parser::parser::NodeList,
    ) -> Option<String> {
        super::emit_utils::first_await_default_param_name(self.arena, &params.nodes)
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

    fn generator_yield_operand_to_ir(&self, idx: NodeIndex) -> IRNode {
        let operand = self.expression_to_ir(idx);
        let Some(comment) = self.yield_operand_line_comment(idx) else {
            return operand;
        };
        let operand_text = crate::transforms::ir_printer::IRPrinter::emit_to_string(&operand);
        IRNode::Raw(format!("\n                {comment}\n                {operand_text}").into())
    }

    fn yield_operand_line_comment(&self, idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(idx)?;
        match node.kind {
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let paren = self.arena.get_parenthesized(node)?;
                let leaf_start = self.expression_leaf_start(paren.expression)?;
                let text = self.source_text?;
                let start = node.pos as usize;
                let end = (leaf_start as usize).min(text.len());
                if start < end {
                    let slice = &text[start..end];
                    if let Some(comment) = slice.lines().rev().find_map(|line| {
                        let trimmed = line.trim_start();
                        trimmed.starts_with("//").then(|| trimmed.to_string())
                    }) {
                        return Some(comment);
                    }
                }
                self.yield_operand_line_comment(paren.expression)
            }
            k if k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                let assertion = self.arena.get_type_assertion(node)?;
                self.yield_operand_line_comment(assertion.expression)
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                let unary = self.arena.get_unary_expr_ex(node)?;
                self.yield_operand_line_comment(unary.expression)
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                let binary = self.arena.get_binary_expr(node)?;
                self.yield_operand_line_comment(binary.left)
                    .or_else(|| self.yield_operand_line_comment(binary.right))
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                let conditional = self.arena.get_conditional_expr(node)?;
                self.yield_operand_line_comment(conditional.condition)
                    .or_else(|| self.yield_operand_line_comment(conditional.when_true))
                    .or_else(|| self.yield_operand_line_comment(conditional.when_false))
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                let access = self.arena.get_access_expr(node)?;
                self.yield_operand_line_comment(access.expression)
                    .or_else(|| self.yield_operand_line_comment(access.name_or_argument))
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                let call = self.arena.get_call_expr(node)?;
                self.yield_operand_line_comment(call.expression)
                    .or_else(|| {
                        call.arguments.as_ref().and_then(|args| {
                            args.nodes
                                .iter()
                                .find_map(|&arg| self.yield_operand_line_comment(arg))
                        })
                    })
            }
            k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => {
                let tagged = self.arena.get_tagged_template(node)?;
                self.yield_operand_line_comment(tagged.tag)
            }
            _ => None,
        }
    }

    fn expression_leaf_start(&self, idx: NodeIndex) -> Option<u32> {
        let node = self.arena.get(idx)?;
        match node.kind {
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let paren = self.arena.get_parenthesized(node)?;
                self.expression_leaf_start(paren.expression)
            }
            k if k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                let assertion = self.arena.get_type_assertion(node)?;
                self.expression_leaf_start(assertion.expression)
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                let unary = self.arena.get_unary_expr_ex(node)?;
                self.expression_leaf_start(unary.expression)
            }
            _ => Some(node.pos),
        }
    }
    /// Extract `VarDecl` names from a `GeneratorBody` IR node and remove them
    /// from the case statements. Returns variable groups to hoist.
    ///
    /// tsc hoists `var` declarations to before the `return __generator(...)` call,
    /// so they appear at the top of the `__awaiter` wrapper function body.
    pub fn extract_and_remove_var_decl_groups(generator_body: &mut IRNode) -> Vec<Vec<String>> {
        let IRNode::GeneratorBody { cases, .. } = generator_body else {
            return Vec::new();
        };

        let mut hoisted = Vec::new();
        let mut current_group = Vec::new();
        for case in cases.iter_mut() {
            let mut i = 0;
            while i < case.statements.len() {
                match &case.statements[i] {
                    IRNode::HoistedVarGroupBreak => {
                        if !current_group.is_empty() {
                            hoisted.push(std::mem::take(&mut current_group));
                        }
                        case.statements.remove(i);
                        continue;
                    }
                    IRNode::VarDecl { name, initializer } if initializer.is_none() => {
                        // Pure declaration with no initializer -- hoist and remove.
                        current_group.push(name.to_string());
                        case.statements.remove(i);
                        continue;
                    }
                    IRNode::VarDecl { name, initializer } => {
                        // Has initializer -- hoist the name but keep as assignment.
                        let var_name = name.clone();
                        current_group.push(var_name.to_string());
                        let init = initializer
                            .clone()
                            .expect("VarDecl match without guard guarantees initializer is Some");
                        case.statements[i] =
                            IRNode::ExpressionStatement(Box::new(IRNode::BinaryExpr {
                                left: Box::new(IRNode::Identifier(var_name)),
                                operator: "=".to_string().into(),
                                right: init,
                            }));
                    }
                    _ => {}
                }
                i += 1;
            }
        }

        if !current_group.is_empty() {
            hoisted.push(current_group);
        }

        hoisted
    }

    pub fn extract_and_remove_var_decls(generator_body: &mut IRNode) -> Vec<String> {
        Self::extract_and_remove_var_decl_groups(generator_body)
            .into_iter()
            .flatten()
            .collect()
    }
}

#[cfg(test)]
#[path = "../../tests/async_es5_ir.rs"]
mod tests;
