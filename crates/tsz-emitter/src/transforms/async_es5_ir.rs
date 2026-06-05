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

use std::cell::{Cell, RefCell};

use crate::transforms::helpers::HelpersNeeded;
use crate::transforms::ir::{IRGeneratorCase, IRNode, IRParam};
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::common::ModuleKind;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;

#[path = "async_es5_ir_bindings.rs"]
mod bindings;
#[path = "async_es5_ir_calls.rs"]
mod calls;
#[path = "async_es5_ir_cases.rs"]
mod cases;
#[path = "async_es5_ir_condition_await.rs"]
mod condition_await;
#[path = "async_es5_ir_control.rs"]
mod control;
#[path = "async_es5_ir_discovery.rs"]
mod discovery;
#[path = "async_es5_ir_disposables.rs"]
mod disposables;
#[path = "async_es5_ir_element_access.rs"]
mod element_access;
#[path = "async_es5_ir_for_await.rs"]
mod for_await;
#[path = "async_es5_ir_for_of.rs"]
mod for_of;
#[path = "async_es5_ir_generator_fn.rs"]
mod generator_fn;
#[path = "async_es5_ir_hoists.rs"]
mod hoists;
#[path = "async_es5_ir_loop_control.rs"]
mod loop_control;
#[path = "async_es5_ir_names.rs"]
mod names;
#[path = "async_es5_ir_state.rs"]
mod state;
#[path = "async_es5_ir_statement_helpers.rs"]
mod statement_helpers;
#[path = "async_es5_ir_statements.rs"]
mod statements;
#[path = "async_es5_ir_suspension.rs"]
mod suspension;
#[path = "async_es5_ir_switch.rs"]
mod switch;
#[path = "async_es5_ir_try_region.rs"]
mod try_region;
#[path = "async_es5_ir_try_statement.rs"]
mod try_statement;

pub use state::AsyncTransformState;

#[path = "async_es5_ir_opcodes.rs"]
pub mod opcodes;

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
    /// Whether ES5 `for..of` lowering must use iterator protocol helpers.
    pub(crate) downlevel_iteration: bool,
    temp_var_counter: Cell<u32>,
    blocked_temp_names: RefCell<FxHashSet<String>>,
    disposable_env_counter: Cell<u32>,
    blocked_disposable_env_names: FxHashSet<String>,
    generated_disposable_env_names: Vec<String>,
    lexical_this_capture: Cell<bool>,
    capture_this_references: Cell<bool>,
    loop_exit_placeholder_counter: Cell<u32>,
    /// Pending hoisted-temp names accumulated by IR-conversion lowerings
    /// (nullish coalescing, optional chaining, etc.) so callers can declare
    /// them in the surrounding state-machine scope. Drained by every
    /// `transform_*` entry point after the generator body is built.
    pub(super) pending_lowering_hoists: RefCell<Vec<String>>,
    /// Whether this async body is emitted inside a derived ES5 class method.
    pub(super) class_has_super: bool,
    /// Generated super parameter name for the surrounding ES5 class IIFE.
    pub(super) class_super_name: String,
    /// Whether the surrounding class member is static.
    pub(super) class_super_is_static: bool,
    /// Module kind for dynamic `import()` lowering inside generator bodies.
    pub(super) module_kind: ModuleKind,
    /// Whether the emit target is ES5. Controls arrow-vs-`function` form in
    /// dynamic-import lowering inside async generator bodies.
    pub(super) target_es5: bool,
    /// Counter for AMD/UMD dynamic import promise callback identifiers.
    pub(in crate::transforms) dynamic_import_promise_counter: Cell<u32>,
    /// Active async-lowered loop labels and the generator label that implements
    /// `continue <label>` for that loop.
    pub(super) labeled_continue_targets: Vec<(String, u32)>,
    /// Active async-lowered loop labels and the generator label that implements
    /// `break <label>` for that loop.
    pub(super) labeled_break_targets: Vec<(String, u32)>,
    /// Active catch binding substitutions used while lowering async try regions.
    pub(super) catch_binding_renames: Vec<(String, String)>,
    /// File-wide ordinal counter for each source catch-binding name.
    /// tsc increments the suffix across async functions in the same file so
    /// the first `catch (e)` in the file becomes `e_1`, the second becomes
    /// `e_2`, and so on, regardless of function boundaries.
    pub(super) catch_binding_ordinals: RefCell<rustc_hash::FxHashMap<String, u32>>,
    /// Catch-binding temps reserved for the async function body currently being
    /// lowered. Reserving them before body conversion lets nested async function
    /// expressions continue after the outer body's catch names.
    pub(super) planned_catch_binding_temps: RefCell<FxHashMap<u32, String>>,
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
            downlevel_iteration: false,
            temp_var_counter: Cell::new(0),
            blocked_temp_names: RefCell::new(FxHashSet::default()),
            disposable_env_counter: Cell::new(1),
            blocked_disposable_env_names: FxHashSet::default(),
            generated_disposable_env_names: Vec::new(),
            lexical_this_capture: Cell::new(false),
            capture_this_references: Cell::new(false),
            loop_exit_placeholder_counter: Cell::new(0),
            pending_lowering_hoists: RefCell::new(Vec::new()),
            class_has_super: false,
            class_super_name: "_super".to_string(),
            class_super_is_static: false,
            module_kind: ModuleKind::None,
            target_es5: false,
            dynamic_import_promise_counter: Cell::new(1),
            labeled_continue_targets: Vec::new(),
            labeled_break_targets: Vec::new(),
            catch_binding_renames: Vec::new(),
            catch_binding_ordinals: RefCell::new(rustc_hash::FxHashMap::default()),
            planned_catch_binding_temps: RefCell::new(FxHashMap::default()),
        }
    }

    /// Record a hoisted-temp name produced by an IR-conversion lowering
    /// (`??`, `?.`, etc.) so the surrounding `transform_*` entry point can
    /// declare it alongside the rest of the state-machine var hoists.
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
        // Function declarations inside async function bodies are always hoisted to
        // the __awaiter callback scope (before __generator), regardless of whether
        // the body contains await expressions.  This matches tsc behavior.
        if let Some(body_node) = self.arena.get(body_idx)
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

        // Extract directive prologues (e.g. "use strict") from the start of the
        // generator body.  tsc places these inside the __awaiter callback before
        // any var declarations and before __generator, so we pull them out here
        // and pass them to AwaiterCall for correct placement.
        let directives = Self::extract_and_remove_directive_prologue(&mut generator_body);

        // Hoist var declarations from generator cases to the awaiter wrapper scope.
        // In tsc output, var declarations inside async function bodies are placed
        // before `return __generator(...)`, not inside the switch/case statements.
        let hoisted_var_groups = self.extract_hoisted_var_groups(&mut generator_body);

        // Extract promise constructor from return type annotation
        let promise_constructor = self.extract_promise_constructor(type_annotation);

        // Build the awaiter call
        let awaiter_call = IRNode::AwaiterCall {
            this_arg: Box::new(IRNode::This { captured: false }),
            needs_lexical_this_capture: generator_body.contains_captured_this_reference(),
            generator_body: Box::new(generator_body),
            hoisted_var_groups,
            promise_constructor,
            multiline_callback: captures_arguments,
            directives,
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

    pub fn transform_async_function_expression(&mut self, func_idx: NodeIndex) -> IRNode {
        match self.transform_async_function(func_idx) {
            IRNode::FunctionDecl {
                name,
                parameters,
                body,
                ..
            } => IRNode::FunctionExpr {
                name: Some(name),
                parameters,
                body,
                is_expression_body: false,
                body_source_range: None,
            },
            node => node,
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
        let hoisted_var_groups = self.extract_hoisted_var_groups(&mut generator_body);
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

    pub fn transform_generator_body_skipping(
        &mut self,
        body_idx: NodeIndex,
        has_await: bool,
        skipped_statements: &[NodeIndex],
    ) -> IRNode {
        self.state.reset();
        self.reset_loop_exit_placeholders();
        self.state.has_await = has_await;
        self.helpers_needed.generator = true;

        self.state.captures_arguments =
            tsz_parser::syntax::transform_utils::contains_arguments_reference(self.arena, body_idx);
        if self.state.captures_arguments && self.state.arguments_capture_name.is_empty() {
            self.state.arguments_capture_name = self.fresh_arguments_capture_name(body_idx, &[]);
        }

        self.build_generator_body(body_idx, has_await, skipped_statements)
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
                self.process_async_statement_list(
                    &block.statements.nodes,
                    cases,
                    current_statements,
                    current_label,
                    skipped_statements,
                );
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
            let value = if let Some(lowered_object) = self.lower_object_literal_before_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                lowered_object
            } else if let Some(lowered_call) = self.lower_call_callee_before_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                lowered_call
            } else if let Some(lowered_array) = self.lower_array_literal_before_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                lowered_array
            } else if let Some(lowered_access) = self.lower_element_access_object_before_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                lowered_access
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
}

#[cfg(test)]
#[path = "../../tests/async_es5_ir.rs"]
mod tests;
