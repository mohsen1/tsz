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

use crate::transforms::class_es5_ir::ES5ClassTransformer;
use crate::transforms::helpers::HelpersNeeded;
use crate::transforms::ir::{IRCatchClause, IRGeneratorCase, IRNode, IRParam};
use rustc_hash::FxHashSet;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::node_flags;
use tsz_parser::parser::syntax_kind_ext;

#[path = "async_es5_ir_bindings.rs"]
mod bindings;
#[path = "async_es5_ir_condition_await.rs"]
mod condition_await;
#[path = "async_es5_ir_discovery.rs"]
mod discovery;
#[path = "async_es5_ir_loop_control.rs"]
mod loop_control;
#[path = "async_es5_ir_state.rs"]
mod state;
#[path = "async_es5_ir_suspension.rs"]
mod suspension;
#[path = "async_es5_ir_try_region.rs"]
mod try_region;

pub use state::AsyncTransformState;
use state::{ForInAssignmentTarget, ForInSuspendedElementIndex, ForInSuspendedObject};
use try_region::{TryRegionPlaceholders, TryRegionResolution, patch_try_region_placeholders};

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
    /// Counter for AMD/UMD dynamic import promise callback identifiers.
    pub(super) dynamic_import_promise_counter: Cell<u32>,
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
            dynamic_import_promise_counter: Cell::new(1),
        }
    }

    /// Record a hoisted-temp name produced by an IR-conversion lowering
    /// (`??`, `?.`, etc.) so the surrounding `transform_*` entry point can
    /// declare it alongside the rest of the state-machine var hoists.
    pub(super) fn push_lowering_hoist(&self, name: String) {
        self.pending_lowering_hoists.borrow_mut().push(name);
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

    /// Set the module kind so dynamic `import()` calls inside the generator
    /// body are lowered to the appropriate module-system form.
    pub const fn set_module_kind(&mut self, kind: ModuleKind) {
        self.module_kind = kind;
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
        loop {
            let counter = self.temp_var_counter.get();
            let name = if counter < 26 {
                format!("_{}", (b'a' + counter as u8) as char)
            } else {
                format!("_{counter}")
            };
            self.temp_var_counter.set(counter + 1);
            if self.blocked_temp_names.borrow_mut().insert(name.clone()) {
                return name;
            }
        }
    }

    pub(super) fn set_temp_var_counter(&self, counter: u32) {
        self.temp_var_counter.set(counter);
    }

    pub const fn temp_var_counter(&self) -> u32 {
        self.temp_var_counter.get()
    }

    fn reset_temp_name_reservations(&self, body_idx: NodeIndex) {
        let mut blocked_names = Vec::new();
        self.collect_body_binding_names(body_idx, &mut blocked_names);
        *self.blocked_temp_names.borrow_mut() = blocked_names.into_iter().collect();
    }

    fn fresh_reserved_name(&self, preferred: impl Into<String>) -> String {
        let preferred = preferred.into();
        if self
            .blocked_temp_names
            .borrow_mut()
            .insert(preferred.clone())
        {
            return preferred;
        }
        let mut suffix = 1u32;
        loop {
            let candidate = format!("{preferred}_{suffix}");
            if self
                .blocked_temp_names
                .borrow_mut()
                .insert(candidate.clone())
            {
                return candidate;
            }
            suffix += 1;
        }
    }

    pub fn set_disposable_env_context<I>(&mut self, next_id: u32, blocked_names: I)
    where
        I: IntoIterator<Item = String>,
    {
        self.disposable_env_counter.set(next_id);
        self.blocked_disposable_env_names = blocked_names.into_iter().collect();
        self.generated_disposable_env_names.clear();
    }

    pub const fn disposable_env_counter(&self) -> u32 {
        self.disposable_env_counter.get()
    }

    pub fn take_generated_disposable_env_names(&mut self) -> Vec<String> {
        std::mem::take(&mut self.generated_disposable_env_names)
    }

    fn next_disposable_env_names(&mut self) -> (String, String, String) {
        loop {
            let env_id = self.disposable_env_counter.get();
            let env_name = format!("env_{env_id}");
            let error_name = format!("e_{env_id}");
            let result_name = format!("result_{env_id}");
            self.disposable_env_counter.set(env_id + 1);

            if self.blocked_disposable_env_names.contains(&env_name)
                || self.blocked_disposable_env_names.contains(&error_name)
                || self.blocked_disposable_env_names.contains(&result_name)
            {
                continue;
            }

            self.blocked_disposable_env_names.insert(env_name.clone());
            self.blocked_disposable_env_names.insert(error_name.clone());
            self.blocked_disposable_env_names
                .insert(result_name.clone());
            self.generated_disposable_env_names.push(env_name.clone());
            self.generated_disposable_env_names.push(error_name.clone());
            self.generated_disposable_env_names
                .push(result_name.clone());
            return (env_name, error_name, result_name);
        }
    }

    fn next_disposable_env_names_allowing_error_gap(&mut self) -> (String, String, String, u32) {
        loop {
            let env_id = self.disposable_env_counter.get();
            let env_name = format!("env_{env_id}");
            let result_name = format!("result_{env_id}");
            self.disposable_env_counter.set(env_id + 1);

            if self.blocked_disposable_env_names.contains(&env_name)
                || self.blocked_disposable_env_names.contains(&result_name)
            {
                continue;
            }

            let mut error_id = env_id;
            loop {
                let error_name = format!("e_{error_id}");
                if self.blocked_disposable_env_names.contains(&error_name) {
                    error_id += 1;
                    continue;
                }

                self.blocked_disposable_env_names.insert(env_name.clone());
                self.blocked_disposable_env_names.insert(error_name.clone());
                self.blocked_disposable_env_names
                    .insert(result_name.clone());
                self.generated_disposable_env_names.push(env_name.clone());
                self.generated_disposable_env_names.push(error_name.clone());
                self.generated_disposable_env_names
                    .push(result_name.clone());
                return (env_name, error_name, result_name, error_id);
            }
        }
    }

    fn env_id_from_name(&self, name: &str) -> Option<u32> {
        name.strip_prefix("env_")?.parse().ok()
    }

    /// Get the helpers needed after transformation
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
        let hoisted_var_groups = self.extract_hoisted_var_groups(&mut generator_body);
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
        self.reset_temp_name_reservations(body_idx);
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

    fn process_async_statement_list(
        &mut self,
        statements: &[NodeIndex],
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
        skipped_statements: &[NodeIndex],
    ) {
        let mut index = 0;
        while index < statements.len() {
            let stmt_idx = statements[index];
            if skipped_statements.contains(&stmt_idx) {
                index += 1;
                continue;
            }
            if self.statement_is_using_variable_statement(stmt_idx) {
                self.process_async_disposable_region(
                    &statements[index..],
                    cases,
                    current_statements,
                    current_label,
                    skipped_statements,
                );
                break;
            }
            self.push_preceding_line_comment(stmt_idx, current_statements);
            self.process_async_statement(stmt_idx, cases, current_statements, current_label);
            index += 1;
        }
    }

    fn push_preceding_line_comment(
        &self,
        stmt_idx: NodeIndex,
        current_statements: &mut Vec<IRNode>,
    ) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let actual_start =
            super::emit_utils::skip_trivia_forward(self.source_text, stmt_node.pos, stmt_node.end);
        if let Some(comment) = self.extract_preceding_line_comment(actual_start) {
            current_statements.push(IRNode::Raw(comment.into()));
        }
    }

    fn statement_is_using_variable_statement(&self, stmt_idx: NodeIndex) -> bool {
        self.using_variable_statement_flags(stmt_idx)
            .is_some_and(|flags| (flags & node_flags::USING) != 0)
    }

    fn using_variable_statement_flags(&self, stmt_idx: NodeIndex) -> Option<u32> {
        let stmt_node = self.arena.get(stmt_idx)?;
        if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
            return None;
        }
        let var_stmt = self.arena.get_variable(stmt_node)?;
        var_stmt
            .declarations
            .nodes
            .iter()
            .find_map(|&decl_list_idx| {
                self.arena.get(decl_list_idx).and_then(|decl_list_node| {
                    ((decl_list_node.flags as u32 & node_flags::USING) != 0)
                        .then_some(decl_list_node.flags as u32)
                })
            })
    }

    fn process_async_disposable_region(
        &mut self,
        statements: &[NodeIndex],
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
        skipped_statements: &[NodeIndex],
    ) {
        let (env_name, error_name, result_name) = self.next_disposable_env_names();
        let using_async = self.statement_slice_has_await_using(statements, skipped_statements);
        let using_binding_names = self.collect_using_binding_names(statements, skipped_statements);
        let start_label = self.state.next_label();
        let try_push_placeholder = u32::MAX;

        current_statements.push(IRNode::VarDecl {
            name: env_name.clone().into(),
            initializer: Some(Box::new(self.disposable_env_initializer())),
        });
        for name in using_binding_names {
            current_statements.push(IRNode::VarDecl {
                name: name.into(),
                initializer: None,
            });
        }
        current_statements.push(IRNode::VarDecl {
            name: error_name.clone().into(),
            initializer: None,
        });
        // Only hoist `result_N` when the region awaits disposal. For pure-`using`
        // regions tsc emits `__disposeResources(env_N);` as a plain expression
        // statement and never assigns to `result_N`, so it never declares the
        // variable either.
        if using_async {
            current_statements.push(IRNode::VarDecl {
                name: result_name.clone().into(),
                initializer: None,
            });
        }
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::GeneratorLabel,
            IRNode::number(start_label.to_string()),
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });
        *current_label = start_label;

        current_statements.push(IRNode::GeneratorTryPush {
            start_label,
            catch_label: try_push_placeholder,
            finally_label: try_push_placeholder,
            end_label: try_push_placeholder,
        });

        for &stmt_idx in statements {
            if skipped_statements.contains(&stmt_idx) {
                continue;
            }
            self.push_preceding_line_comment(stmt_idx, current_statements);
            if self.statement_is_using_variable_statement(stmt_idx) {
                self.process_using_variable_statement_in_region(
                    stmt_idx,
                    &env_name,
                    current_statements,
                );
            } else {
                self.process_async_statement(stmt_idx, cases, current_statements, current_label);
            }
        }

        let catch_label = self.state.next_label();
        let finally_label = self.state.next_label();
        let dispose_resume_label = using_async.then(|| self.state.next_label());
        let dispose_done_label = if using_async {
            self.state.next_label()
        } else {
            finally_label
        };
        let end_label = self.state.next_label();
        Self::patch_generator_try_push(
            cases,
            current_statements,
            start_label,
            catch_label,
            finally_label,
            end_label,
        );

        current_statements.push(Self::generator_break_statement(end_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = catch_label;
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(error_name.clone()),
            IRNode::GeneratorSent,
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::prop(IRNode::id(env_name.clone()), "error"),
            IRNode::id(error_name),
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::prop(IRNode::id(env_name.clone()), "hasError"),
            IRNode::BooleanLiteral(true),
        ))));
        current_statements.push(Self::generator_break_statement(end_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = finally_label;
        // For pure-`using` regions tsc emits the dispose call as a bare
        // expression statement; only `await using` regions need the
        // `result_N = __disposeResources(env_N);` capture so the value can be
        // awaited before endfinally.
        let dispose_call = IRNode::CallExpr {
            callee: Box::new(IRNode::RuntimeHelper("__disposeResources".into())),
            arguments: vec![IRNode::id(env_name)],
        };
        if using_async {
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(result_name.clone()),
                dispose_call,
            ))));
        } else {
            current_statements.push(IRNode::ExpressionStatement(Box::new(dispose_call)));
        }

        if using_async {
            current_statements.push(IRNode::IfBreak {
                condition: Box::new(IRNode::PrefixUnaryExpr {
                    operator: "!".into(),
                    operand: Box::new(IRNode::id(result_name.clone())),
                }),
                target_label: dispose_done_label,
            });
            let dispose_yield_value = if self.async_generator_mode {
                IRNode::CallExpr {
                    callee: Box::new(IRNode::RuntimeHelper("__await".into())),
                    arguments: vec![IRNode::id(result_name)],
                }
            } else {
                IRNode::id(result_name)
            };
            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::YIELD,
                    value: Some(Box::new(dispose_yield_value)),
                    comment: Some("yield".into()),
                },
            ))));
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            *current_label =
                dispose_resume_label.expect("async disposable regions reserve a resume label");
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::GeneratorSent)));
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::GeneratorLabel,
                IRNode::number(dispose_done_label.to_string()),
            ))));
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
        }

        *current_label = dispose_done_label;
        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
            IRNode::GeneratorOp {
                opcode: opcodes::END_FINALLY,
                value: None,
                comment: Some("endfinally".into()),
            },
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });
        *current_label = end_label;
    }

    fn disposable_env_initializer(&self) -> IRNode {
        IRNode::object(vec![
            crate::transforms::ir::IRProperty {
                key: crate::transforms::ir::IRPropertyKey::Identifier("stack".into()),
                value: IRNode::ArrayLiteral(Vec::new()),
                kind: crate::transforms::ir::IRPropertyKind::Init,
            },
            crate::transforms::ir::IRProperty {
                key: crate::transforms::ir::IRPropertyKey::Identifier("error".into()),
                value: IRNode::Undefined,
                kind: crate::transforms::ir::IRPropertyKind::Init,
            },
            crate::transforms::ir::IRProperty {
                key: crate::transforms::ir::IRPropertyKey::Identifier("hasError".into()),
                value: IRNode::BooleanLiteral(false),
                kind: crate::transforms::ir::IRPropertyKind::Init,
            },
        ])
    }

    fn add_disposable_resource_call(
        &self,
        env_name: &str,
        value_name: &str,
        using_async: bool,
    ) -> IRNode {
        IRNode::CallExpr {
            callee: Box::new(IRNode::RuntimeHelper("__addDisposableResource".into())),
            arguments: vec![
                IRNode::id(env_name.to_string()),
                IRNode::id(value_name.to_string()),
                IRNode::BooleanLiteral(using_async),
            ],
        }
    }

    fn generator_break_statement(target_label: u32) -> IRNode {
        IRNode::ReturnStatement(Some(Box::new(IRNode::GeneratorOp {
            opcode: opcodes::BREAK,
            value: Some(Box::new(IRNode::NumericLiteral(
                target_label.to_string().into(),
            ))),
            comment: Some("break".into()),
        })))
    }

    fn patch_generator_try_push(
        cases: &mut [IRGeneratorCase],
        current_statements: &mut [IRNode],
        start_label: u32,
        catch_label: u32,
        finally_label: u32,
        end_label: u32,
    ) {
        for case in cases {
            Self::patch_generator_try_push_in_statements(
                &mut case.statements,
                start_label,
                catch_label,
                finally_label,
                end_label,
            );
        }
        Self::patch_generator_try_push_in_statements(
            current_statements,
            start_label,
            catch_label,
            finally_label,
            end_label,
        );
    }

    fn patch_generator_try_push_in_statements(
        statements: &mut [IRNode],
        start_label: u32,
        catch_label: u32,
        finally_label: u32,
        end_label: u32,
    ) {
        for statement in statements {
            if let IRNode::GeneratorTryPush {
                start_label: candidate_start,
                catch_label: candidate_catch,
                finally_label: candidate_finally,
                end_label: candidate_end,
            } = statement
                && *candidate_start == start_label
                && *candidate_catch == u32::MAX
            {
                *candidate_catch = catch_label;
                *candidate_finally = finally_label;
                *candidate_end = end_label;
            }
        }
    }

    fn statement_slice_has_await_using(
        &self,
        statements: &[NodeIndex],
        skipped_statements: &[NodeIndex],
    ) -> bool {
        statements.iter().copied().any(|stmt_idx| {
            !skipped_statements.contains(&stmt_idx)
                && self
                    .using_variable_statement_flags(stmt_idx)
                    .is_some_and(node_flags::is_await_using)
        })
    }

    fn collect_using_binding_names(
        &self,
        statements: &[NodeIndex],
        skipped_statements: &[NodeIndex],
    ) -> Vec<String> {
        let mut names = Vec::new();
        for &stmt_idx in statements {
            if skipped_statements.contains(&stmt_idx)
                || !self.statement_is_using_variable_statement(stmt_idx)
            {
                continue;
            }
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
                continue;
            };
            for &decl_list_idx in &var_stmt.declarations.nodes {
                let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                    continue;
                };
                if (decl_list_node.flags as u32 & node_flags::USING) == 0 {
                    continue;
                }
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
                    let name = crate::transforms::emit_utils::identifier_text_or_empty(
                        self.arena, decl.name,
                    );
                    if !name.is_empty() && !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
        }
        names
    }

    fn process_using_variable_statement_in_region(
        &mut self,
        stmt_idx: NodeIndex,
        env_name: &str,
        current_statements: &mut Vec<IRNode>,
    ) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
            return;
        };
        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            if (decl_list_node.flags as u32 & node_flags::USING) == 0 {
                continue;
            }
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                continue;
            };
            for &decl_idx in &decl_list.declarations.nodes {
                self.process_using_variable_declaration_in_region(
                    decl_idx,
                    env_name,
                    node_flags::is_await_using(decl_list_node.flags as u32),
                    current_statements,
                );
            }
        }
    }

    fn process_using_variable_declaration_in_region(
        &mut self,
        decl_idx: NodeIndex,
        env_name: &str,
        using_async: bool,
        current_statements: &mut Vec<IRNode>,
    ) {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return;
        };
        let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
            return;
        };
        let name = crate::transforms::emit_utils::identifier_text_or_empty(self.arena, decl.name);
        if name.is_empty() {
            return;
        }
        let value = if decl.initializer.is_none() {
            IRNode::Undefined
        } else if let Some((temp, lowered_init)) =
            self.lower_object_literal_es5_with_computed_properties(decl.initializer)
        {
            current_statements.push(IRNode::HoistedVarGroupBreak);
            current_statements.push(IRNode::VarDecl {
                name: temp.into(),
                initializer: None,
            });
            lowered_init
        } else {
            self.expression_to_ir(decl.initializer)
        };
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(name),
            IRNode::CallExpr {
                callee: Box::new(IRNode::RuntimeHelper("__addDisposableResource".into())),
                arguments: vec![
                    IRNode::id(env_name.to_string()),
                    value,
                    IRNode::BooleanLiteral(using_async),
                ],
            },
        ))));
    }

    fn process_for_of_using_statement_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::FOR_OF_STATEMENT {
            return false;
        }
        let Some(for_in_of) = self.arena.get_for_in_of(node) else {
            return false;
        };
        if for_in_of.await_modifier {
            return false;
        }
        let Some(using_info) =
            super::emit_utils::for_of_using_info(self.arena, for_in_of.initializer)
        else {
            return false;
        };

        let env_id = self.disposable_env_counter.get();
        let (env_name, error_name, result_name) = self.next_disposable_env_names();
        let index_name = self.fresh_reserved_name("_i");
        let array_name = self.for_of_iterable_temp_name(for_in_of.expression, env_id);
        let value_temp_name =
            self.fresh_reserved_name(format!("{}_{}", using_info.binding_name, env_id));

        for name in [
            &index_name,
            &array_name,
            &value_temp_name,
            &env_name,
            &using_info.binding_name,
            &error_name,
            &result_name,
        ] {
            current_statements.push(IRNode::VarDecl {
                name: name.to_string().into(),
                initializer: None,
            });
        }

        let iterable = self.for_of_iterable_to_ir_with_es5_computed_temps(
            for_in_of.expression,
            current_statements,
        );
        let loop_label = self.state.next_label();
        let try_start_label = self.state.next_label();
        let catch_label = self.state.next_label();
        let finally_label = self.state.next_label();
        let dispose_resume_label = self.state.next_label();
        let dispose_done_label = self.state.next_label();
        let iteration_label = self.state.next_label();
        let end_label = self.state.next_label();

        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::binary(
            IRNode::assign(IRNode::id(index_name.clone()), IRNode::number("0")),
            ",",
            IRNode::assign(IRNode::id(array_name.clone()), iterable),
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::GeneratorLabel,
            IRNode::number(loop_label.to_string()),
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = loop_label;
        current_statements.push(IRNode::IfBreak {
            condition: Box::new(IRNode::PrefixUnaryExpr {
                operator: "!".into(),
                operand: Box::new(IRNode::Parenthesized(Box::new(IRNode::binary(
                    IRNode::id(index_name.clone()),
                    "<",
                    IRNode::prop(IRNode::id(array_name.clone()), "length"),
                )))),
            }),
            target_label: end_label,
        });
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(value_temp_name.clone()),
            IRNode::elem(IRNode::id(array_name), IRNode::id(index_name.clone())),
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(env_name.clone()),
            self.disposable_env_initializer(),
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::GeneratorLabel,
            IRNode::number(try_start_label.to_string()),
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = try_start_label;
        current_statements.push(IRNode::GeneratorTryPush {
            start_label: try_start_label,
            catch_label,
            finally_label,
            end_label: iteration_label,
        });
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(using_info.binding_name),
            IRNode::CallExpr {
                callee: Box::new(IRNode::RuntimeHelper("__addDisposableResource".into())),
                arguments: vec![
                    IRNode::id(env_name.clone()),
                    IRNode::id(value_temp_name),
                    IRNode::BooleanLiteral(using_info.using_async),
                ],
            },
        ))));
        self.process_block_or_statement_in_async(
            for_in_of.statement,
            cases,
            current_statements,
            current_label,
        );
        current_statements.push(Self::generator_break_statement(iteration_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = catch_label;
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(error_name.clone()),
            IRNode::GeneratorSent,
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::prop(IRNode::id(env_name.clone()), "error"),
            IRNode::id(error_name),
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::prop(IRNode::id(env_name.clone()), "hasError"),
            IRNode::BooleanLiteral(true),
        ))));
        current_statements.push(Self::generator_break_statement(iteration_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = finally_label;
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(result_name.clone()),
            IRNode::CallExpr {
                callee: Box::new(IRNode::RuntimeHelper("__disposeResources".into())),
                arguments: vec![IRNode::id(env_name)],
            },
        ))));
        current_statements.push(IRNode::IfBreak {
            condition: Box::new(IRNode::PrefixUnaryExpr {
                operator: "!".into(),
                operand: Box::new(IRNode::id(result_name.clone())),
            }),
            target_label: dispose_done_label,
        });
        let dispose_yield_value = if self.async_generator_mode {
            IRNode::CallExpr {
                callee: Box::new(IRNode::RuntimeHelper("__await".into())),
                arguments: vec![IRNode::id(result_name)],
            }
        } else {
            IRNode::id(result_name)
        };
        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
            IRNode::GeneratorOp {
                opcode: opcodes::YIELD,
                value: Some(Box::new(dispose_yield_value)),
                comment: Some("yield".into()),
            },
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = dispose_resume_label;
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::GeneratorSent)));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::GeneratorLabel,
            IRNode::number(dispose_done_label.to_string()),
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = dispose_done_label;
        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
            IRNode::GeneratorOp {
                opcode: opcodes::END_FINALLY,
                value: None,
                comment: Some("endfinally".into()),
            },
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = iteration_label;
        current_statements.push(IRNode::ExpressionStatement(Box::new(
            IRNode::PostfixUnaryExpr {
                operand: Box::new(IRNode::id(index_name)),
                operator: "++".into(),
            },
        )));
        current_statements.push(Self::generator_break_statement(loop_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = end_label;
        true
    }

    fn process_for_await_using_statement_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::FOR_OF_STATEMENT {
            return false;
        }
        let Some(for_in_of) = self.arena.get_for_in_of(node) else {
            return false;
        };
        if !for_in_of.await_modifier {
            return false;
        }
        let Some(using_info) =
            super::emit_utils::for_of_using_info(self.arena, for_in_of.initializer)
        else {
            return false;
        };

        self.helpers_needed.mark_async_values();
        self.helpers_needed.add_disposable_resource = true;
        self.helpers_needed.dispose_resources = true;

        let loop_guard_name = self.generate_hoisted_temp();
        let env_id = self.disposable_env_counter.get();
        let (iterator_name, result_name) =
            self.for_await_iterator_names(for_in_of.expression, env_id);
        let binding_name = if using_info.recovered_missing_binding {
            self.generate_hoisted_temp()
        } else {
            using_info.binding_name
        };
        let value_binding_name = format!("{binding_name}_1");

        let (env_name, resource_error_name, dispose_result_name, resource_error_id) =
            if using_info.using_async {
                let (env_name, error_name, result_name, error_id) =
                    self.next_disposable_env_names_allowing_error_gap();
                (env_name, error_name, Some(result_name), error_id)
            } else {
                let env_id = self.disposable_env_counter.get();
                self.disposable_env_counter.set(env_id + 1);
                let env_name = format!("env_{env_id}");
                self.blocked_disposable_env_names.insert(env_name.clone());
                self.generated_disposable_env_names.push(env_name.clone());
                (env_name, format!("e_{}", env_id + 1), None, env_id + 1)
            };

        let outer_error_id = if using_info.using_async {
            resource_error_id + 1
        } else {
            self.env_id_from_name(&env_name).unwrap_or(1)
        };
        let outer_error_name = format!("e_{outer_error_id}");
        let outer_catch_error_name = format!("{outer_error_name}_1");

        for name in [
            loop_guard_name.as_str(),
            iterator_name.as_str(),
            result_name.as_str(),
            value_binding_name.as_str(),
            env_name.as_str(),
            binding_name.as_str(),
        ] {
            current_statements.push(IRNode::var_decl(name.to_string(), None));
        }
        if using_info.using_async {
            current_statements.push(IRNode::var_decl(resource_error_name.clone(), None));
            if let Some(dispose_result_name) = &dispose_result_name {
                current_statements.push(IRNode::var_decl(dispose_result_name.clone(), None));
            }
        }
        current_statements.push(IRNode::var_decl(outer_catch_error_name.clone(), None));

        let iterable = self.for_of_iterable_to_ir_with_es5_computed_temps(
            for_in_of.expression,
            current_statements,
        );

        current_statements.push(IRNode::HoistedVarGroupBreak);
        let done_name = self.generate_hoisted_temp();
        let return_name = self.generate_hoisted_temp();
        let value_name = self.generate_hoisted_temp();
        for name in [&done_name, &outer_error_name, &return_name, &value_name] {
            current_statements.push(IRNode::var_decl(name.clone(), None));
        }

        let loop_yield_label = self.state.next_label();
        let after_next_label = self.state.next_label();
        let (
            resource_start_label,
            resource_catch_label,
            resource_finally_label,
            dispose_resume_label,
            dispose_done_label,
            iteration_label,
            loop_exit_label,
        ) = if using_info.using_async {
            (
                self.state.next_label(),
                self.state.next_label(),
                self.state.next_label(),
                Some(self.state.next_label()),
                Some(self.state.next_label()),
                self.state.next_label(),
                self.state.next_label(),
            )
        } else {
            (
                u32::MAX,
                u32::MAX,
                u32::MAX,
                None,
                None,
                self.state.next_label(),
                self.state.next_label(),
            )
        };
        let outer_catch_label = self.state.next_label();
        let outer_finally_label = self.state.next_label();
        let return_resume_label = self.state.next_label();
        let return_done_label = self.state.next_label();
        let rethrow_label = self.state.next_label();
        let outer_endfinally_label = self.state.next_label();
        let end_label = self.state.next_label();

        current_statements.push(IRNode::GeneratorTryPush {
            start_label: *current_label,
            catch_label: outer_catch_label,
            finally_label: outer_finally_label,
            end_label,
        });
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::binary(
            IRNode::assign(
                IRNode::id(loop_guard_name.clone()),
                IRNode::BooleanLiteral(true),
            ),
            ",",
            IRNode::assign(
                IRNode::id(iterator_name.clone()),
                IRNode::CallExpr {
                    callee: Box::new(IRNode::RuntimeHelper("__asyncValues".into())),
                    arguments: vec![iterable],
                },
            ),
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::GeneratorLabel,
            IRNode::number(loop_yield_label.to_string()),
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = loop_yield_label;
        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
            IRNode::GeneratorOp {
                opcode: opcodes::YIELD,
                value: Some(Box::new(IRNode::CallExpr {
                    callee: Box::new(IRNode::prop(IRNode::id(iterator_name.clone()), "next")),
                    arguments: vec![],
                })),
                comment: Some("yield".into()),
            },
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = after_next_label;
        current_statements.push(IRNode::IfBreak {
            condition: Box::new(IRNode::PrefixUnaryExpr {
                operator: "!".into(),
                operand: Box::new(IRNode::CommaExpr(vec![
                    IRNode::assign(IRNode::id(result_name.clone()), IRNode::GeneratorSent),
                    IRNode::assign(
                        IRNode::id(done_name.clone()),
                        IRNode::prop(IRNode::id(result_name.clone()), "done"),
                    ),
                    IRNode::PrefixUnaryExpr {
                        operator: "!".into(),
                        operand: Box::new(IRNode::id(done_name.clone())),
                    },
                ])),
            }),
            target_label: loop_exit_label,
        });
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(value_name.clone()),
            IRNode::prop(IRNode::id(result_name), "value"),
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(loop_guard_name.clone()),
            IRNode::BooleanLiteral(false),
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(value_binding_name.clone()),
            IRNode::id(value_name),
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(env_name.clone()),
            self.disposable_env_initializer(),
        ))));

        if using_info.using_async {
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::GeneratorLabel,
                IRNode::number(resource_start_label.to_string()),
            ))));
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            *current_label = resource_start_label;
            current_statements.push(IRNode::GeneratorTryPush {
                start_label: resource_start_label,
                catch_label: resource_catch_label,
                finally_label: resource_finally_label,
                end_label: iteration_label,
            });
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(binding_name),
                self.add_disposable_resource_call(
                    &env_name,
                    &value_binding_name,
                    using_info.using_async,
                ),
            ))));
            self.process_block_or_statement_in_async(
                for_in_of.statement,
                cases,
                current_statements,
                current_label,
            );
            current_statements.push(Self::generator_break_statement(iteration_label));
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            *current_label = resource_catch_label;
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(resource_error_name.clone()),
                IRNode::GeneratorSent,
            ))));
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::prop(IRNode::id(env_name.clone()), "error"),
                IRNode::id(resource_error_name),
            ))));
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::prop(IRNode::id(env_name.clone()), "hasError"),
                IRNode::BooleanLiteral(true),
            ))));
            current_statements.push(Self::generator_break_statement(iteration_label));
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            let dispose_result_name =
                dispose_result_name.expect("await using reserves a dispose result");
            *current_label = resource_finally_label;
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(dispose_result_name.clone()),
                IRNode::CallExpr {
                    callee: Box::new(IRNode::RuntimeHelper("__disposeResources".into())),
                    arguments: vec![IRNode::id(env_name)],
                },
            ))));
            current_statements.push(IRNode::IfBreak {
                condition: Box::new(IRNode::PrefixUnaryExpr {
                    operator: "!".into(),
                    operand: Box::new(IRNode::id(dispose_result_name.clone())),
                }),
                target_label: dispose_done_label.expect("await using reserves done label"),
            });
            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::YIELD,
                    value: Some(Box::new(IRNode::id(dispose_result_name))),
                    comment: Some("yield".into()),
                },
            ))));
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            *current_label = dispose_resume_label.expect("await using reserves resume label");
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::GeneratorSent)));
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::GeneratorLabel,
                IRNode::number(
                    dispose_done_label
                        .expect("await using reserves done label")
                        .to_string(),
                ),
            ))));
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            *current_label = dispose_done_label.expect("await using reserves done label");
            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::END_FINALLY,
                    value: None,
                    comment: Some("endfinally".into()),
                },
            ))));
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
        } else {
            current_statements.push(IRNode::TryStatement {
                try_block: Box::new(IRNode::Block(vec![IRNode::ExpressionStatement(Box::new(
                    IRNode::assign(
                        IRNode::id(binding_name),
                        self.add_disposable_resource_call(
                            &env_name,
                            &value_binding_name,
                            using_info.using_async,
                        ),
                    ),
                ))])),
                catch_clause: Some(IRCatchClause {
                    param: Some(resource_error_name.into()),
                    body: vec![
                        IRNode::ExpressionStatement(Box::new(IRNode::assign(
                            IRNode::prop(IRNode::id(env_name.clone()), "error"),
                            IRNode::id(format!(
                                "e_{}",
                                self.env_id_from_name(&env_name).unwrap_or(1) + 1
                            )),
                        ))),
                        IRNode::ExpressionStatement(Box::new(IRNode::assign(
                            IRNode::prop(IRNode::id(env_name.clone()), "hasError"),
                            IRNode::BooleanLiteral(true),
                        ))),
                    ],
                }),
                finally_block: Some(Box::new(IRNode::Block(vec![IRNode::ExpressionStatement(
                    Box::new(IRNode::CallExpr {
                        callee: Box::new(IRNode::RuntimeHelper("__disposeResources".into())),
                        arguments: vec![IRNode::id(env_name)],
                    }),
                )]))),
            });
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::GeneratorLabel,
                IRNode::number(iteration_label.to_string()),
            ))));
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
        }

        *current_label = iteration_label;
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(loop_guard_name.clone()),
            IRNode::BooleanLiteral(true),
        ))));
        current_statements.push(Self::generator_break_statement(loop_yield_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = loop_exit_label;
        current_statements.push(Self::generator_break_statement(end_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = outer_catch_label;
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(outer_catch_error_name.clone()),
            IRNode::GeneratorSent,
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(outer_error_name.clone()),
            IRNode::object(vec![crate::transforms::ir::IRProperty {
                key: crate::transforms::ir::IRPropertyKey::Identifier("error".into()),
                value: IRNode::id(outer_catch_error_name),
                kind: crate::transforms::ir::IRPropertyKind::Init,
            }]),
        ))));
        current_statements.push(Self::generator_break_statement(end_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = outer_finally_label;
        current_statements.push(IRNode::GeneratorTryPushFinally {
            start_label: outer_finally_label,
            finally_label: rethrow_label,
            end_label: outer_endfinally_label,
        });
        current_statements.push(IRNode::IfBreak {
            condition: Box::new(IRNode::PrefixUnaryExpr {
                operator: "!".into(),
                operand: Box::new(IRNode::Parenthesized(Box::new(IRNode::logical_and(
                    IRNode::logical_and(
                        IRNode::PrefixUnaryExpr {
                            operator: "!".into(),
                            operand: Box::new(IRNode::id(loop_guard_name)),
                        },
                        IRNode::PrefixUnaryExpr {
                            operator: "!".into(),
                            operand: Box::new(IRNode::id(done_name)),
                        },
                    ),
                    IRNode::Parenthesized(Box::new(IRNode::assign(
                        IRNode::id(return_name.clone()),
                        IRNode::prop(IRNode::id(iterator_name.clone()), "return"),
                    ))),
                )))),
            }),
            target_label: return_done_label,
        });
        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
            IRNode::GeneratorOp {
                opcode: opcodes::YIELD,
                value: Some(Box::new(IRNode::CallExpr {
                    callee: Box::new(IRNode::prop(IRNode::id(return_name), "call")),
                    arguments: vec![IRNode::id(iterator_name)],
                })),
                comment: Some("yield".into()),
            },
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = return_resume_label;
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::GeneratorSent)));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::GeneratorLabel,
            IRNode::number(return_done_label.to_string()),
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = return_done_label;
        current_statements.push(Self::generator_break_statement(outer_endfinally_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = rethrow_label;
        current_statements.push(IRNode::IfStatement {
            condition: Box::new(IRNode::id(outer_error_name.clone())),
            then_branch: Box::new(IRNode::ThrowStatement(Box::new(IRNode::prop(
                IRNode::id(outer_error_name),
                "error",
            )))),
            else_branch: None,
        });
        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
            IRNode::GeneratorOp {
                opcode: opcodes::END_FINALLY,
                value: None,
                comment: Some("endfinally".into()),
            },
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = outer_endfinally_label;
        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
            IRNode::GeneratorOp {
                opcode: opcodes::END_FINALLY,
                value: None,
                comment: Some("endfinally".into()),
            },
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = end_label;
        true
    }

    fn process_for_initializer_using_statement_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::FOR_STATEMENT {
            return false;
        }
        let Some(loop_data) = self.arena.get_loop(node) else {
            return false;
        };
        let Some((using_async, declarations)) =
            self.for_initializer_using_declarations(loop_data.initializer)
        else {
            return false;
        };

        self.helpers_needed.add_disposable_resource = true;
        self.helpers_needed.dispose_resources = true;

        let (env_name, error_name, result_name) = self.next_disposable_env_names();
        current_statements.push(IRNode::var_decl(env_name.clone(), None));
        for &decl_idx in &declarations {
            if let Some(name) = self.variable_declaration_name(decl_idx) {
                current_statements.push(IRNode::var_decl(name, None));
            }
        }
        current_statements.push(IRNode::var_decl(error_name.clone(), None));
        if using_async {
            current_statements.push(IRNode::var_decl(result_name.clone(), None));
        }

        let mut registration_exprs = Vec::new();
        let mut started_computed_temp_group = false;
        for &decl_idx in &declarations {
            let Some(name) = self.variable_declaration_name(decl_idx) else {
                continue;
            };
            let value = self.using_declaration_initializer_value(
                decl_idx,
                current_statements,
                &mut started_computed_temp_group,
            );
            registration_exprs.push(IRNode::assign(
                IRNode::id(name),
                IRNode::CallExpr {
                    callee: Box::new(IRNode::RuntimeHelper("__addDisposableResource".into())),
                    arguments: vec![
                        IRNode::id(env_name.clone()),
                        value,
                        IRNode::BooleanLiteral(using_async),
                    ],
                },
            ));
        }

        let start_label = self.state.next_label();
        let catch_label = self.state.next_label();
        let finally_label = self.state.next_label();
        let dispose_resume_label = self.state.next_label();
        let dispose_done_label = self.state.next_label();
        let end_label = self.state.next_label();

        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(env_name.clone()),
            self.disposable_env_initializer(),
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::GeneratorLabel,
            IRNode::number(start_label.to_string()),
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = start_label;
        current_statements.push(IRNode::GeneratorTryPush {
            start_label,
            catch_label,
            finally_label,
            end_label,
        });
        if let Some(registration_expr) = Self::comma_chain(registration_exprs) {
            current_statements.push(IRNode::ExpressionStatement(Box::new(registration_expr)));
        }
        current_statements.push(IRNode::ForStatement {
            initializer: None,
            condition: loop_data
                .condition
                .is_some()
                .then(|| Box::new(self.expression_to_ir(loop_data.condition))),
            incrementor: loop_data
                .incrementor
                .is_some()
                .then(|| Box::new(self.expression_to_ir(loop_data.incrementor))),
            body: Box::new(self.loop_body_to_ir(loop_data.statement)),
        });
        current_statements.push(Self::generator_break_statement(end_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = catch_label;
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(error_name.clone()),
            IRNode::GeneratorSent,
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::prop(IRNode::id(env_name.clone()), "error"),
            IRNode::id(error_name),
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::prop(IRNode::id(env_name.clone()), "hasError"),
            IRNode::BooleanLiteral(true),
        ))));
        current_statements.push(Self::generator_break_statement(end_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = finally_label;
        if using_async {
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(result_name.clone()),
                IRNode::CallExpr {
                    callee: Box::new(IRNode::RuntimeHelper("__disposeResources".into())),
                    arguments: vec![IRNode::id(env_name)],
                },
            ))));
            current_statements.push(IRNode::IfBreak {
                condition: Box::new(IRNode::PrefixUnaryExpr {
                    operator: "!".into(),
                    operand: Box::new(IRNode::id(result_name.clone())),
                }),
                target_label: dispose_done_label,
            });
            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::YIELD,
                    value: Some(Box::new(IRNode::id(result_name))),
                    comment: Some("yield".into()),
                },
            ))));
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            *current_label = dispose_resume_label;
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::GeneratorSent)));
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::GeneratorLabel,
                IRNode::number(dispose_done_label.to_string()),
            ))));
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
        } else {
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::CallExpr {
                callee: Box::new(IRNode::RuntimeHelper("__disposeResources".into())),
                arguments: vec![IRNode::id(env_name)],
            })));
        }

        *current_label = dispose_done_label;
        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
            IRNode::GeneratorOp {
                opcode: opcodes::END_FINALLY,
                value: None,
                comment: Some("endfinally".into()),
            },
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = end_label;
        true
    }

    fn for_initializer_using_declarations(
        &self,
        initializer: NodeIndex,
    ) -> Option<(bool, Vec<NodeIndex>)> {
        let init_node = self.arena.get(initializer)?;
        let flags = init_node.flags as u32;
        if (flags & node_flags::USING) == 0 && !node_flags::is_await_using(flags) {
            return None;
        }
        let decl_list = self.arena.get_variable(init_node)?;
        Some((
            node_flags::is_await_using(flags),
            decl_list.declarations.nodes.clone(),
        ))
    }

    fn variable_declaration_name(&self, decl_idx: NodeIndex) -> Option<String> {
        let decl_node = self.arena.get(decl_idx)?;
        let decl = self.arena.get_variable_declaration(decl_node)?;
        let name = super::emit_utils::identifier_text_or_empty(self.arena, decl.name);
        (!name.is_empty()).then_some(name)
    }

    fn loop_body_to_ir(&self, statement: NodeIndex) -> IRNode {
        let Some(node) = self.arena.get(statement) else {
            return IRNode::EmptyStatement;
        };
        if node.kind != syntax_kind_ext::BLOCK {
            return self.statement_to_ir(statement);
        }
        let Some(block) = self.arena.get_block(node) else {
            return IRNode::Block(Vec::new());
        };
        IRNode::Block(
            block
                .statements
                .nodes
                .iter()
                .map(|&stmt| self.statement_to_ir(stmt))
                .collect(),
        )
    }

    fn using_declaration_initializer_value(
        &self,
        decl_idx: NodeIndex,
        current_statements: &mut Vec<IRNode>,
        started_computed_temp_group: &mut bool,
    ) -> IRNode {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return IRNode::Undefined;
        };
        let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
            return IRNode::Undefined;
        };
        if decl.initializer.is_none() {
            return IRNode::Undefined;
        }
        if let Some((temp, lowered)) =
            self.lower_object_literal_es5_with_computed_properties(decl.initializer)
        {
            if !*started_computed_temp_group {
                current_statements.push(IRNode::HoistedVarGroupBreak);
                *started_computed_temp_group = true;
            }
            current_statements.push(IRNode::VarDecl {
                name: temp.into(),
                initializer: None,
            });
            lowered
        } else {
            self.expression_to_ir(decl.initializer)
        }
    }

    fn comma_chain(mut expressions: Vec<IRNode>) -> Option<IRNode> {
        if expressions.is_empty() {
            return None;
        }
        let mut expression = expressions.remove(0);
        for next in expressions {
            expression = IRNode::binary(expression, ",", next);
        }
        Some(expression)
    }

    fn for_of_iterable_temp_name(&self, expression: NodeIndex, env_id: u32) -> String {
        if let Some(expr_node) = self.arena.get(expression)
            && expr_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
        {
            let name = super::emit_utils::identifier_text_or_empty(self.arena, expression);
            if !name.is_empty() {
                return self.fresh_reserved_name(format!("{name}_{env_id}"));
            }
        }
        self.generate_hoisted_temp()
    }

    fn for_await_iterator_names(&self, expression: NodeIndex, env_id: u32) -> (String, String) {
        if let Some(expr_node) = self.arena.get(expression)
            && expr_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
        {
            let name = super::emit_utils::identifier_text_or_empty(self.arena, expression);
            if !name.is_empty() {
                let iterator_name = format!("{name}_{env_id}");
                return (iterator_name.clone(), format!("{iterator_name}_1"));
            }
        }
        (self.generate_hoisted_temp(), self.generate_hoisted_temp())
    }

    fn for_of_iterable_to_ir_with_es5_computed_temps(
        &self,
        expression: NodeIndex,
        current_statements: &mut Vec<IRNode>,
    ) -> IRNode {
        let Some(expr_node) = self.arena.get(expression) else {
            return IRNode::Undefined;
        };
        if expr_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return self.expression_to_ir(expression);
        }
        let Some(array) = self.arena.get_literal_expr(expr_node) else {
            return IRNode::ArrayLiteral(Vec::new());
        };

        let mut started_computed_temp_group = false;
        let elements = array
            .elements
            .nodes
            .iter()
            .map(|&element| {
                if let Some((temp, lowered)) =
                    self.lower_object_literal_es5_with_computed_properties(element)
                {
                    if !started_computed_temp_group {
                        current_statements.push(IRNode::HoistedVarGroupBreak);
                        started_computed_temp_group = true;
                    }
                    current_statements.push(IRNode::VarDecl {
                        name: temp.into(),
                        initializer: None,
                    });
                    lowered
                } else {
                    self.expression_to_ir(element)
                }
            })
            .collect();

        IRNode::ArrayLiteral(elements)
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
            k if k == syntax_kind_ext::EMPTY_STATEMENT => {
                current_statements.push(IRNode::EmptyStatement);
            }

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
                if self.lower_class_declaration_to_assignment(idx, current_statements) {
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

            k if k == syntax_kind_ext::DO_STATEMENT => {
                self.process_do_while_statement_in_async(
                    idx,
                    cases,
                    current_statements,
                    current_label,
                );
            }

            k if k == syntax_kind_ext::FOR_STATEMENT => {
                if !self.process_for_initializer_using_statement_in_async(
                    idx,
                    cases,
                    current_statements,
                    current_label,
                ) && !self.process_captured_for_statement_in_async(
                    idx,
                    cases,
                    current_statements,
                    current_label,
                ) && !self.process_for_statement_in_async(
                    idx,
                    cases,
                    current_statements,
                    current_label,
                ) {
                    current_statements.push(self.statement_to_ir(idx));
                }
            }

            k if k == syntax_kind_ext::FOR_IN_STATEMENT => {
                if !self.process_for_in_statement_in_async(
                    idx,
                    cases,
                    current_statements,
                    current_label,
                ) {
                    current_statements.push(self.statement_to_ir(idx));
                }
            }

            k if k == syntax_kind_ext::FOR_OF_STATEMENT => {
                if !self.process_for_await_using_statement_in_async(
                    idx,
                    cases,
                    current_statements,
                    current_label,
                ) && !self.process_for_of_using_statement_in_async(
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

            k if k == syntax_kind_ext::BLOCK => {
                self.process_block_or_statement_in_async(
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
            // Try specialized lowering in priority order before falling back to the
            // generic emit_nested_suspension path.  Each helper handles a specific
            // structural pattern and returns false/None if the pattern doesn't match.

            // `target = base[await index]` — element access with await in index
            if let Some(lowered) = self.lower_element_access_before_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                current_statements.push(IRNode::ExpressionStatement(Box::new(lowered)));
                return;
            }

            // `target = cond ? await T : F` or `target = cond ? T : await F`
            if self.lower_assignment_with_conditional_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                return;
            }

            // `(await lhs) op= await rhs` — compound assignment with await in BOTH sides
            if self.lower_compound_assignment_double_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                return;
            }

            // `lhs op= await rhs` — compound assignment with await in RHS
            if self.lower_compound_assignment_before_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                return;
            }

            // `L OP await R` (non-assignment, non-short-circuit)
            if let Some(lowered) = self.lower_binary_non_short_circuit_before_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                current_statements.push(IRNode::ExpressionStatement(Box::new(lowered)));
                return;
            }

            // `L && await R`, `L || await R`, `L ?? await R`
            if let Some(lowered) = self.lower_logical_short_circuit_before_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                current_statements.push(IRNode::ExpressionStatement(Box::new(lowered)));
                return;
            }

            // Existing handler: property/element assignment target saving
            if self.lower_assignment_target_before_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                return;
            }

            // `obj[await idx] = rhs` or `obj[await idx] op= rhs` — await in LHS index
            if self.lower_lhs_element_access_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                return;
            }

            // `(obj[await idx]).prop = rhs` — property access with await in element index
            if self.lower_lhs_chained_element_access_suspension(
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

    fn lower_class_declaration_to_assignment(
        &mut self,
        idx: NodeIndex,
        current_statements: &mut Vec<IRNode>,
    ) -> bool {
        let mut class_transformer = ES5ClassTransformer::new(self.arena);
        if let Some(source_text) = self.source_text {
            class_transformer.set_source_text(source_text);
        }
        let Some(class_ir) = class_transformer.transform_class_to_ir(idx) else {
            return false;
        };

        let IRNode::ES5ClassIIFE {
            name,
            binding_name: _,
            base_class,
            super_param,
            body,
            weakmap_decls,
            computed_prop_temp_decls,
            computed_prop_temp_inits,
            weakmap_inits,
            leading_comment,
            deferred_static_blocks,
            deferred_block_class_alias,
        } = class_ir
        else {
            return false;
        };

        for decl_name in weakmap_decls
            .into_iter()
            .chain(computed_prop_temp_decls)
            .chain(deferred_block_class_alias.iter().cloned())
            .chain(std::iter::once(name.to_string()))
        {
            current_statements.push(IRNode::VarDecl {
                name: decl_name.into(),
                initializer: None,
            });
        }

        current_statements.push(IRNode::ES5ClassAssignment {
            name,
            base_class,
            super_param,
            body,
            computed_prop_temp_inits,
            weakmap_inits,
            leading_comment,
            deferred_static_blocks,
            deferred_block_class_alias,
        });

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
        let class_ir =
            class_transformer.transform_class_to_ir_with_name(class_idx, Some(class_name))?;
        let IRNode::ES5ClassIIFE {
            binding_name: _,
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

        let cond_has_await = self.contains_await_recursive(if_stmt.expression);
        let then_has_await = self.contains_await_recursive(if_stmt.then_statement);
        let else_has_await = if_stmt.else_statement.is_some()
            && self.contains_await_recursive(if_stmt.else_statement);

        if !cond_has_await && !then_has_await && !else_has_await {
            // No await anywhere in this if statement -- emit as-is
            let ir = self.statement_to_ir(idx);
            current_statements.push(ir);
            return;
        }

        // When the condition itself is or contains an await expression, yield the
        // condition first and use _a.sent() as the condition for the branch.
        // When no branch contains await but the condition does, we still need to
        // split cases around the yield.
        let cond_ir = if self.is_suspension_expression(if_stmt.expression) {
            // Condition IS directly an await expression: yield it, then check sent()
            self.process_await_expression(
                if_stmt.expression,
                cases,
                current_statements,
                current_label,
            );
            IRNode::GeneratorSent
        } else if cond_has_await {
            // Condition contains nested await: emit the suspension first
            self.emit_nested_suspension(
                if_stmt.expression,
                cases,
                current_statements,
                current_label,
            );
            self.expression_to_ir(if_stmt.expression)
        } else {
            self.expression_to_ir(if_stmt.expression)
        };

        if !then_has_await && !else_has_await {
            // Only the condition had await; the branches are await-free so emit a
            // simple if statement using the (now-resolved) condition IR value.
            let has_else = if_stmt.else_statement.is_some()
                && self
                    .arena
                    .get(if_stmt.else_statement)
                    .is_some_and(|n| n.kind != syntax_kind_ext::EMPTY_STATEMENT);
            let then_ir = self.statement_to_ir(if_stmt.then_statement);
            let else_ir = if has_else {
                Some(Box::new(self.statement_to_ir(if_stmt.else_statement)))
            } else {
                None
            };
            current_statements.push(IRNode::IfStatement {
                condition: Box::new(cond_ir),
                then_branch: Box::new(then_ir),
                else_branch: else_ir,
            });
            return;
        }

        let has_else = if_stmt.else_statement.is_some()
            && self
                .arena
                .get(if_stmt.else_statement)
                .is_some_and(|n| n.kind != syntax_kind_ext::EMPTY_STATEMENT);

        // Label allocation strategy:
        //
        // We need three logical labels:
        //   else_label  – where the else branch begins (or end_label when no else)
        //   end_label   – the merge point after both branches
        //
        // The problem: branches that contain `await` consume extra labels when they
        // are processed. Pre-allocating a label too early causes collisions with
        // the labels the branch allocates internally.
        //
        // Solution: use placeholders (MAX - counter) for labels that must be
        // allocated AFTER a suspending branch is processed, then patch them.
        //
        // Rules:
        //  - When then_has_await: else_label must be delayed (then branch allocates
        //    its yield-resume label first).
        //  - When either branch has await: end_label must be delayed (the awaiting
        //    branch allocates its yield-resume label, which must precede end_label).
        //
        // Non-awaiting branches that fall through to end_label need an explicit
        // `_a.label = end_label` assignment so the state machine advances correctly
        // on re-entry.

        let delayed_else_label = has_else && then_has_await;
        let delayed_end_label = then_has_await || else_has_await;

        let else_placeholder = delayed_else_label.then(|| self.next_loop_exit_placeholder());
        let end_placeholder = delayed_end_label.then(|| self.next_loop_exit_placeholder());

        let mut else_label: Option<u32> = if delayed_else_label {
            None
        } else {
            Some(self.state.next_label())
        };
        let mut end_label: Option<u32> = if delayed_end_label {
            None
        } else {
            // No branch suspends: both else_label and end_label are safe to allocate now.
            if has_else {
                Some(self.state.next_label())
            } else {
                // No else: end_label == else_label (the next case after the then block)
                else_label
            }
        };

        // Emit: if (!(condition)) return [3 /*break*/, else_or_end_placeholder];
        // - When there's an else branch: skip to else_label (or its placeholder).
        // - When no else branch: skip to end_label (or its placeholder).
        let branch_skip_target = if has_else {
            else_placeholder.unwrap_or_else(|| {
                else_label.expect("else label must be allocated when not delayed")
            })
        } else {
            end_placeholder.unwrap_or_else(|| {
                end_label.expect("end label must be allocated when not delayed and no else")
            })
        };
        current_statements.push(IRNode::IfBreak {
            condition: Box::new(IRNode::PrefixUnaryExpr {
                operator: "!".to_string().into(),
                operand: Box::new(cond_ir),
            }),
            target_label: branch_skip_target,
        });

        // Process then branch
        self.process_block_or_statement_in_async(
            if_stmt.then_statement,
            cases,
            current_statements,
            current_label,
        );

        if has_else {
            // Allocate else_label (and possibly end_label) now that then has been processed.
            if let Some(placeholder) = else_placeholder {
                let patched_else_label = self.state.next_label();
                Self::patch_if_break_target(cases, placeholder, patched_else_label);
                Self::patch_if_break_target_in_statements(
                    current_statements,
                    placeholder,
                    patched_else_label,
                );
                else_label = Some(patched_else_label);
            }
            // If end_label is also delayed and then_has_await, allocate it now (after
            // then-branch labels are consumed) but before the else branch runs.
            // When else_has_await, end_label must wait until after the else branch.
            if let Some(end_ph) = end_placeholder
                && !else_has_await
            {
                let patched_end_label = self.state.next_label();
                Self::patch_if_break_target(cases, end_ph, patched_end_label);
                Self::patch_if_break_target_in_statements(
                    current_statements,
                    end_ph,
                    patched_end_label,
                );
                end_label = Some(patched_end_label);
            }

            let else_l = else_label.expect("else label must be available before else branch");
            let end_l_or_ph = end_label.unwrap_or_else(|| {
                end_placeholder.expect("end placeholder must exist when end_label not yet resolved")
            });

            // Emit: return [3 /*break*/, end_label]; at end of then branch
            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::BREAK,
                    value: Some(Box::new(IRNode::NumericLiteral(
                        end_l_or_ph.to_string().into(),
                    ))),
                    comment: Some("break".to_string().into()),
                },
            ))));

            // Flush current case and start else branch
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
            *current_label = else_l;

            // Process else branch
            self.process_block_or_statement_in_async(
                if_stmt.else_statement,
                cases,
                current_statements,
                current_label,
            );

            // Allocate end_label after the else branch if it was delayed.
            if let Some(end_ph) = end_placeholder
                && else_has_await
            {
                let patched_end_label = self.state.next_label();
                Self::patch_if_break_target(cases, end_ph, patched_end_label);
                Self::patch_if_break_target_in_statements(
                    current_statements,
                    end_ph,
                    patched_end_label,
                );
                end_label = Some(patched_end_label);
            }
            let end_l = end_label.expect("end label must be resolved after else branch");

            // Emit `_a.label = end_label` so the state machine falls through
            // correctly to the merge point on re-entry.  This is needed whenever
            // the last case of the else branch does not already return/break:
            //  - Else branch with no await: statements end without a return.
            //  - Else branch with await: after the yield-resume, `_a.sent()` is
            //    in current_statements and the generator needs the label hint.
            if !current_statements.is_empty()
                && !matches!(
                    current_statements.last(),
                    Some(
                        IRNode::ReturnStatement(_)
                            | IRNode::ThrowStatement(_)
                            | IRNode::BreakStatement(_)
                    )
                )
            {
                current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                    IRNode::GeneratorLabel,
                    IRNode::number(end_l.to_string()),
                ))));
            }

            // Flush current case and start end label
            if !current_statements.is_empty() {
                cases.push(IRGeneratorCase {
                    label: *current_label,
                    statements: std::mem::take(current_statements),
                });
            }
            *current_label = end_l;
        } else {
            // No else branch.
            // Flush current case and start end label
            if !current_statements.is_empty() {
                cases.push(IRGeneratorCase {
                    label: *current_label,
                    statements: std::mem::take(current_statements),
                });
            }
            *current_label = end_label.expect("end label must be available after if lowering");
        }
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

    fn process_for_in_statement_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::FOR_IN_STATEMENT {
            return false;
        }
        let Some(for_in) = self.arena.get_for_in_of(node) else {
            return false;
        };

        let initializer_has_suspension = self.contains_await_recursive(for_in.initializer);
        let expression_has_suspension = self.contains_await_recursive(for_in.expression);
        let body_has_suspension = self.contains_await_recursive(for_in.statement);
        if !initializer_has_suspension && !expression_has_suspension && !body_has_suspension {
            return self.process_simple_for_in_statement(for_in, current_statements);
        }
        if self.for_in_body_has_unsupported_control_flow(for_in.statement) {
            return false;
        }

        let object_suspension = self.direct_suspension_expression(for_in.expression);
        if expression_has_suspension && object_suspension.is_none() {
            return false;
        }

        let Some((assignment_target, declared_iteration_name)) =
            self.for_in_assignment_target(for_in.initializer)
        else {
            return false;
        };

        let object_temp = self.generate_hoisted_temp();
        let keys_temp = self.generate_hoisted_temp();
        let key_temp = self.generate_hoisted_temp();
        let index_temp = self.fresh_reserved_name("_i");
        let target_object_temp = if matches!(
            assignment_target,
            ForInAssignmentTarget::SuspendedElement {
                index: ForInSuspendedElementIndex::Suspended(_),
                ..
            }
        ) {
            Some(self.generate_hoisted_temp())
        } else {
            None
        };

        for name in [&object_temp, &keys_temp, &key_temp, &index_temp] {
            current_statements.push(IRNode::VarDecl {
                name: name.clone().into(),
                initializer: None,
            });
        }
        if let Some(temp) = &target_object_temp {
            current_statements.push(IRNode::VarDecl {
                name: temp.clone().into(),
                initializer: None,
            });
        }
        if let Some(iteration_name) = declared_iteration_name {
            current_statements.push(IRNode::VarDecl {
                name: iteration_name.into(),
                initializer: None,
            });
        }

        let object_value = if let Some(suspension) = object_suspension {
            self.process_await_expression(suspension, cases, current_statements, current_label);
            IRNode::GeneratorSent
        } else {
            self.expression_to_ir(for_in.expression)
        };

        current_statements.push(Self::expression_statement(IRNode::assign(
            IRNode::id(object_temp.clone()),
            object_value,
        )));
        current_statements.push(Self::expression_statement(IRNode::assign(
            IRNode::id(keys_temp.clone()),
            IRNode::ArrayLiteral(Vec::new()),
        )));
        current_statements.push(IRNode::ForInOfStatement {
            kind: "in".into(),
            initializer: Box::new(IRNode::id(key_temp.clone())),
            expression: Box::new(IRNode::id(object_temp.clone())),
            body: Box::new(Self::expression_statement(IRNode::CallExpr {
                callee: Box::new(IRNode::prop(IRNode::id(keys_temp.clone()), "push")),
                arguments: vec![IRNode::id(key_temp.clone())],
            })),
            multiline_body: true,
        });
        current_statements.push(Self::expression_statement(IRNode::assign(
            IRNode::id(index_temp.clone()),
            IRNode::number("0"),
        )));

        let loop_label = self.state.next_label();
        let increment_placeholder = self.next_loop_exit_placeholder();
        let end_placeholder = self.next_loop_exit_placeholder();
        current_statements.push(Self::generator_label_assignment(loop_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });
        *current_label = loop_label;

        current_statements.push(IRNode::IfBreak {
            condition: Box::new(IRNode::PrefixUnaryExpr {
                operator: "!".into(),
                operand: Box::new(IRNode::Parenthesized(Box::new(IRNode::binary(
                    IRNode::id(index_temp.clone()),
                    "<",
                    IRNode::prop(IRNode::id(keys_temp.clone()), "length"),
                )))),
            }),
            target_label: end_placeholder,
        });
        current_statements.push(Self::expression_statement(IRNode::assign(
            IRNode::id(key_temp.clone()),
            IRNode::elem(IRNode::id(keys_temp), IRNode::id(index_temp.clone())),
        )));
        current_statements.push(IRNode::IfBreak {
            condition: Box::new(IRNode::PrefixUnaryExpr {
                operator: "!".into(),
                operand: Box::new(IRNode::Parenthesized(Box::new(IRNode::binary(
                    IRNode::id(key_temp.clone()),
                    "in",
                    IRNode::id(object_temp),
                )))),
            }),
            target_label: increment_placeholder,
        });
        match assignment_target {
            ForInAssignmentTarget::Direct(target) => {
                current_statements.push(Self::expression_statement(IRNode::assign(
                    *target,
                    IRNode::id(key_temp),
                )));
            }
            ForInAssignmentTarget::SuspendedProperty {
                object_suspension,
                property,
            } => {
                self.process_await_expression(
                    object_suspension,
                    cases,
                    current_statements,
                    current_label,
                );
                current_statements.push(Self::expression_statement(IRNode::assign(
                    IRNode::prop(
                        IRNode::Parenthesized(Box::new(IRNode::GeneratorSent)),
                        property,
                    ),
                    IRNode::id(key_temp),
                )));
            }
            ForInAssignmentTarget::SuspendedElement { object, index } => match index {
                ForInSuspendedElementIndex::Direct(index) => {
                    let ForInSuspendedObject::Suspended(object_suspension) = object else {
                        return false;
                    };
                    self.process_await_expression(
                        object_suspension,
                        cases,
                        current_statements,
                        current_label,
                    );
                    current_statements.push(Self::expression_statement(IRNode::assign(
                        IRNode::elem(
                            IRNode::Parenthesized(Box::new(IRNode::GeneratorSent)),
                            *index,
                        ),
                        IRNode::id(key_temp),
                    )));
                }
                ForInSuspendedElementIndex::Suspended(index_suspension) => {
                    let Some(temp) = target_object_temp else {
                        return false;
                    };
                    match object {
                        ForInSuspendedObject::Direct(object) => {
                            current_statements.push(Self::expression_statement(IRNode::assign(
                                IRNode::id(temp.clone()),
                                *object,
                            )));
                        }
                        ForInSuspendedObject::Suspended(object_suspension) => {
                            self.process_await_expression(
                                object_suspension,
                                cases,
                                current_statements,
                                current_label,
                            );
                            current_statements.push(Self::expression_statement(IRNode::assign(
                                IRNode::id(temp.clone()),
                                IRNode::Parenthesized(Box::new(IRNode::GeneratorSent)),
                            )));
                        }
                    }
                    self.process_await_expression(
                        index_suspension,
                        cases,
                        current_statements,
                        current_label,
                    );
                    current_statements.push(Self::expression_statement(IRNode::assign(
                        IRNode::elem(IRNode::id(temp), IRNode::GeneratorSent),
                        IRNode::id(key_temp),
                    )));
                }
            },
        }

        self.process_block_or_statement_in_async(
            for_in.statement,
            cases,
            current_statements,
            current_label,
        );

        let increment_label = self.state.next_label();
        let end_label = self.state.next_label();
        current_statements.push(Self::generator_label_assignment(increment_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        current_statements.push(Self::expression_statement(IRNode::PostfixUnaryExpr {
            operand: Box::new(IRNode::id(index_temp)),
            operator: "++".into(),
        }));
        current_statements.push(Self::generator_break_statement(loop_label));
        cases.push(IRGeneratorCase {
            label: increment_label,
            statements: std::mem::take(current_statements),
        });

        Self::patch_if_break_target(cases, increment_placeholder, increment_label);
        Self::patch_if_break_target(cases, end_placeholder, end_label);
        *current_label = end_label;
        true
    }

    fn process_simple_for_in_statement(
        &self,
        for_in: &tsz_parser::parser::node::ForInOfData,
        current_statements: &mut Vec<IRNode>,
    ) -> bool {
        let Some((target, declared_iteration_name)) =
            self.for_in_direct_assignment_target(for_in.initializer)
        else {
            return false;
        };
        if let Some(iteration_name) = declared_iteration_name {
            current_statements.push(IRNode::VarDecl {
                name: iteration_name.into(),
                initializer: None,
            });
        }
        current_statements.push(IRNode::ForInOfStatement {
            kind: "in".into(),
            initializer: Box::new(target),
            expression: Box::new(self.expression_to_ir(for_in.expression)),
            body: Box::new(self.statement_to_ir(for_in.statement)),
            multiline_body: false,
        });
        true
    }

    fn for_in_assignment_target(
        &self,
        initializer: NodeIndex,
    ) -> Option<(ForInAssignmentTarget, Option<String>)> {
        if self.contains_await_recursive(initializer) {
            return self
                .for_in_suspended_assignment_target(initializer)
                .map(|target| (target, None));
        }
        self.for_in_direct_assignment_target(initializer)
            .map(|(target, declared_name)| {
                (
                    ForInAssignmentTarget::Direct(Box::new(target)),
                    declared_name,
                )
            })
    }

    fn for_in_direct_assignment_target(
        &self,
        initializer: NodeIndex,
    ) -> Option<(IRNode, Option<String>)> {
        let init_node = self.arena.get(initializer)?;
        if init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            let decl_list = self.arena.get_variable(init_node)?;
            if decl_list.declarations.nodes.len() != 1 {
                return None;
            }
            let decl_idx = *decl_list.declarations.nodes.first()?;
            let decl_node = self.arena.get(decl_idx)?;
            let decl = self.arena.get_variable_declaration(decl_node)?;
            if decl.initializer.is_some() {
                return None;
            }
            let name = super::emit_utils::identifier_text(self.arena, decl.name)?;
            return Some((IRNode::id(name.clone()), Some(name)));
        }
        if init_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            let name = super::emit_utils::identifier_text(self.arena, initializer)?;
            return Some((IRNode::id(name), None));
        }
        if init_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || init_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return Some((self.expression_to_ir(initializer), None));
        }
        None
    }

    fn for_in_suspended_assignment_target(
        &self,
        initializer: NodeIndex,
    ) -> Option<ForInAssignmentTarget> {
        let initializer = self.strip_parenthesized_expression(initializer);
        let init_node = self.arena.get(initializer)?;
        if init_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(init_node)?;
            let object_suspension = self.direct_suspension_expression(access.expression)?;
            let property = crate::transforms::emit_utils::identifier_text_or_empty(
                self.arena,
                access.name_or_argument,
            );
            return Some(ForInAssignmentTarget::SuspendedProperty {
                object_suspension,
                property,
            });
        }
        if init_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(init_node)?;
            let object = if let Some(object_suspension) =
                self.direct_suspension_expression(access.expression)
            {
                ForInSuspendedObject::Suspended(object_suspension)
            } else if self.contains_await_recursive(access.expression) {
                return None;
            } else {
                ForInSuspendedObject::Direct(Box::new(self.expression_to_ir(access.expression)))
            };
            let index = if let Some(index_suspension) =
                self.direct_suspension_expression(access.name_or_argument)
            {
                ForInSuspendedElementIndex::Suspended(index_suspension)
            } else if self.contains_await_recursive(access.name_or_argument) {
                return None;
            } else {
                ForInSuspendedElementIndex::Direct(Box::new(
                    self.expression_to_ir(access.name_or_argument),
                ))
            };
            if matches!(object, ForInSuspendedObject::Direct(_))
                && matches!(index, ForInSuspendedElementIndex::Direct(_))
            {
                return None;
            }
            return Some(ForInAssignmentTarget::SuspendedElement { object, index });
        }
        None
    }

    fn direct_suspension_expression(&self, expression: NodeIndex) -> Option<NodeIndex> {
        let expression = self.strip_parenthesized_expression(expression);
        self.is_suspension_expression(expression)
            .then_some(expression)
    }

    fn for_in_body_has_unsupported_control_flow(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            || node.is_function_expression_or_arrow()
        {
            return false;
        }
        match node.kind {
            k if k == syntax_kind_ext::BREAK_STATEMENT
                || k == syntax_kind_ext::CONTINUE_STATEMENT
                || k == syntax_kind_ext::RETURN_STATEMENT =>
            {
                true
            }
            k if k == syntax_kind_ext::BLOCK || k == syntax_kind_ext::CASE_BLOCK => {
                self.arena.get_block(node).is_some_and(|block| {
                    block
                        .statements
                        .nodes
                        .iter()
                        .any(|&stmt| self.for_in_body_has_unsupported_control_flow(stmt))
                })
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                self.arena.get_if_statement(node).is_some_and(|if_stmt| {
                    self.for_in_body_has_unsupported_control_flow(if_stmt.then_statement)
                        || self.for_in_body_has_unsupported_control_flow(if_stmt.else_statement)
                })
            }
            k if k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::DO_STATEMENT
                || k == syntax_kind_ext::FOR_STATEMENT
                || k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::FOR_OF_STATEMENT
                || k == syntax_kind_ext::SWITCH_STATEMENT
                || k == syntax_kind_ext::TRY_STATEMENT
                || k == syntax_kind_ext::LABELED_STATEMENT =>
            {
                true
            }
            _ => false,
        }
    }

    fn expression_statement(expression: IRNode) -> IRNode {
        IRNode::ExpressionStatement(Box::new(expression))
    }

    fn negated_condition(condition: IRNode) -> IRNode {
        let operand = match condition {
            IRNode::BinaryExpr { .. }
            | IRNode::LogicalOr { .. }
            | IRNode::LogicalAnd { .. }
            | IRNode::ConditionalExpr { .. }
            | IRNode::CommaExpr(_)
            | IRNode::CommaExprMultiline(_) => IRNode::Parenthesized(Box::new(condition)),
            _ => condition,
        };
        IRNode::PrefixUnaryExpr {
            operator: "!".into(),
            operand: Box::new(operand),
        }
    }

    fn generator_label_assignment(label: u32) -> IRNode {
        Self::expression_statement(IRNode::assign(
            IRNode::GeneratorLabel,
            IRNode::number(label.to_string()),
        ))
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

        if !has_catch && !has_finally {
            self.process_block_or_statement_in_async(
                try_data.try_block,
                cases,
                current_statements,
                current_label,
            );
            return;
        }

        // Sentinels share `next_loop_exit_placeholder` so the patch sweep cannot
        // collide with loop-exit placeholders still living in a surrounding loop.
        let placeholders = TryRegionPlaceholders {
            catch_slot: self.next_loop_exit_placeholder(),
            finally_slot: self.next_loop_exit_placeholder(),
            end_slot: self.next_loop_exit_placeholder(),
            exit_break: self.next_loop_exit_placeholder(),
        };
        let start_label = *current_label;
        let cases_start = cases.len();

        current_statements.push(IRNode::generator_try_push(
            start_label,
            has_catch.then_some(placeholders.catch_slot),
            has_finally.then_some(placeholders.finally_slot),
            placeholders.end_slot,
        ));

        self.process_block_or_statement_in_async(
            try_data.try_block,
            cases,
            current_statements,
            current_label,
        );
        current_statements.push(Self::generator_break_statement(placeholders.exit_break));

        let catch_label = if has_catch {
            let cl = self.state.next_label();
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
            *current_label = cl;

            if let Some(catch_node) = self.arena.get(try_data.catch_clause)
                && let Some(catch_data) = self.arena.get_catch_clause(catch_node)
            {
                if catch_data.variable_declaration.is_some() {
                    let catch_var_name =
                        self.get_catch_variable_name(catch_data.variable_declaration);
                    if !catch_var_name.is_empty() {
                        // tsc binds the exception via `_a.sent()`, not `_a[1]`.
                        current_statements.push(IRNode::ExpressionStatement(Box::new(
                            IRNode::assign(IRNode::id(catch_var_name), IRNode::GeneratorSent),
                        )));
                    }
                }
                self.process_block_or_statement_in_async(
                    catch_data.block,
                    cases,
                    current_statements,
                    current_label,
                );
            }

            current_statements.push(Self::generator_break_statement(placeholders.exit_break));
            Some(cl)
        } else {
            None
        };

        let finally_label = if has_finally {
            let fl = self.state.next_label();
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
            *current_label = fl;

            self.process_block_or_statement_in_async(
                try_data.finally_block,
                cases,
                current_statements,
                current_label,
            );

            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::END_FINALLY,
                    value: None,
                    comment: Some("endfinally".to_string().into()),
                },
            ))));
            Some(fl)
        } else {
            None
        };

        // End label is allocated last so its number is past every interior resume.
        let end_label = self.state.next_label();

        let resolution = TryRegionResolution {
            placeholders,
            catch_label,
            finally_label,
            end_label,
            // Breaks from try/catch must target the region's end label even when
            // a finally exists; tsc's `__generator` driver detects the active try
            // entry on a `[3 /*break*/, end]` op, pushes the pending break onto
            // `_.ops`, then jumps to the finally label. After `[7 /*endfinally*/]`
            // pops `_.ops`, the driver resumes the original break against an
            // empty `_.trys` stack and lands at `end`. Breaking directly to the
            // finally label would jump there without pushing onto `_.ops`, so
            // `endfinally` would pop an empty stack and the state machine would
            // wedge.
            exit_target: end_label,
        };
        let cases_tail = cases[cases_start..]
            .iter_mut()
            .flat_map(|case| case.statements.iter_mut())
            .chain(current_statements.iter_mut());
        for stmt in cases_tail {
            patch_try_region_placeholders(stmt, &resolution);
        }

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
                self.process_async_statement_list(
                    &block.statements.nodes,
                    cases,
                    current_statements,
                    current_label,
                    &[],
                );
            }
        } else {
            self.process_async_statement(idx, cases, current_statements, current_label);
        }
    }

    // =========================================================================
    // Helper methods
    // =========================================================================

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
    /// Extract leading directive prologues (e.g. `"use strict"`) from the first
    /// case of a generator body and return them as raw string values (without quotes).
    ///
    /// When a directive appears at the top of an async function body, `tsc` places
    /// it inside the `__awaiter` callback — before any `var` declarations and
    /// before `__generator` — not inside the switch/case statements.  This helper
    /// removes those nodes from case 0 and returns their string content so that
    /// the `AwaiterCall` printer can emit them in the correct position.
    ///
    /// Handles `StringLiteral`, `RawStringLiteral`, and `Raw` nodes (the last form
    /// is emitted when the source text is available and the value is a quoted token).
    pub fn extract_and_remove_directive_prologue(generator_body: &mut IRNode) -> Vec<String> {
        let IRNode::GeneratorBody { cases, .. } = generator_body else {
            return Vec::new();
        };
        let Some(first_case) = cases.first_mut() else {
            return Vec::new();
        };
        let mut directives = Vec::new();
        while let Some(IRNode::ExpressionStatement(expr)) = first_case.statements.first() {
            let directive = match expr.as_ref() {
                IRNode::StringLiteral(text) | IRNode::RawStringLiteral(text) => {
                    // text is already the inner value (no quotes)
                    text.to_string()
                }
                IRNode::Raw(raw) => {
                    // Raw nodes produced from source tokens include the surrounding quotes.
                    // Accept quoted string tokens that look like directive prologues.
                    let trimmed = raw.trim();
                    if (trimmed.starts_with('"') && trimmed.ends_with('"'))
                        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
                    {
                        // Strip quotes to get the inner value
                        trimmed[1..trimmed.len() - 1].to_string()
                    } else {
                        break;
                    }
                }
                _ => break,
            };
            directives.push(directive);
            first_case.statements.remove(0);
        }
        directives
    }

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
