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
//!
//! # Architecture
//!
//! This module uses the IR-based transformation pattern:
//! - `AsyncES5Transformer` (in `async_es5_ir.rs`) produces IR nodes
//! - `AsyncES5Emitter` is a thin wrapper that uses `IRPrinter` to emit JavaScript
//!
//! This separation allows clean transform logic while delegating string emission
//! to the centralized `IRPrinter`.

use crate::transforms::async_es5_ir::AsyncES5Transformer;
use crate::transforms::ir::IRNode;
use crate::transforms::ir_printer::IRPrinter;
use crate::transforms::tslib_helper_naming::TslibHelperNaming;
use tsz_common::common::ModuleKind;
use tsz_common::source_map::Mapping;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;

// Re-export from async_es5_ir for backward compatibility
pub use crate::transforms::async_es5_ir::{AsyncTransformState, opcodes};

/// Async ES5 emitter for transforming async functions.
///
/// This is a thin wrapper around `AsyncES5Transformer` that uses `IRPrinter`
/// to emit JavaScript strings. It provides the same API as the legacy emitter
/// for backward compatibility.
pub struct AsyncES5Emitter<'a> {
    arena: &'a NodeArena,
    transformer: AsyncES5Transformer<'a>,
    indent_level: u32,
    source_text: Option<&'a str>,
    source_index: u32,
    mappings: Vec<Mapping>,
    /// When true, the inner `IRPrinter` records source mappings for re-emitted
    /// `ASTRef` nodes so the downleveled body is mapped. Set by the caller only
    /// when a source map is being generated.
    capture_mappings: bool,
    this_capture_depth: u32,
    class_name: Option<String>,
    /// Outer names (e.g. a class-expression alias) that must not be chosen as
    /// the generator state variable.  When non-empty they are treated as
    /// already-allocated hoisted vars for the purpose of `generator_state_name_for_hoisted`,
    /// so the state-name picker skips past them.
    outer_reserved_for_generator_state: Vec<String>,
    /// Naming of runtime helpers under `importHelpers` (CommonJS prefix / ESM alias).
    tslib_helpers: TslibHelperNaming,
    system_import_meta: bool,
    generator_this_arg: String,
}

impl<'a> AsyncES5Emitter<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        Self {
            arena,
            transformer: AsyncES5Transformer::new(arena),
            indent_level: 0,
            source_text: None,
            source_index: 0,
            mappings: Vec::new(),
            capture_mappings: false,
            this_capture_depth: 0,
            class_name: None,
            outer_reserved_for_generator_state: Vec::new(),
            tslib_helpers: TslibHelperNaming::default(),
            system_import_meta: false,
            generator_this_arg: "this".to_string(),
        }
    }

    pub const fn set_indent_level(&mut self, level: u32) {
        self.indent_level = level;
    }

    pub const fn set_tslib_prefix(&mut self, enable: bool) {
        self.tslib_helpers.set_prefix(enable);
    }

    pub fn set_tslib_import_binding(&mut self, binding: String) {
        self.tslib_helpers.set_binding(binding);
    }

    /// Set per-file helper import renames (e.g. `__awaiter` -> `__awaiter_1`).
    pub fn set_helper_import_aliases(&mut self, aliases: rustc_hash::FxHashMap<String, String>) {
        self.tslib_helpers.set_aliases(aliases);
    }

    pub const fn set_system_import_meta(&mut self, enabled: bool) {
        self.system_import_meta = enabled;
    }

    pub fn set_generator_this_arg(&mut self, arg: String) {
        self.generator_this_arg = arg;
    }

    pub const fn set_module_kind(&mut self, kind: ModuleKind) {
        self.transformer.set_module_kind(kind);
    }

    pub const fn set_target_es5(&mut self, es5: bool) {
        self.transformer.set_target_es5(es5);
    }

    pub fn set_dynamic_import_promise_counter(&mut self, next_id: u32) {
        self.transformer.dynamic_import_promise_counter.set(next_id);
    }

    pub const fn dynamic_import_promise_counter(&self) -> u32 {
        self.transformer.dynamic_import_promise_counter.get()
    }

    pub fn set_catch_binding_ordinals(&mut self, ordinals: rustc_hash::FxHashMap<String, u32>) {
        self.transformer.set_catch_binding_ordinals(ordinals);
    }

    pub fn take_catch_binding_ordinals(&self) -> rustc_hash::FxHashMap<String, u32> {
        self.transformer.take_catch_binding_ordinals()
    }

    pub const fn set_downlevel_iteration(&mut self, enabled: bool) {
        self.transformer.set_downlevel_iteration(enabled);
    }

    pub fn set_temp_var_counter(&mut self, counter: u32) {
        self.transformer.set_temp_var_counter(counter);
    }

    pub const fn temp_var_counter(&self) -> u32 {
        self.transformer.temp_var_counter()
    }

    pub fn set_disposable_env_context<I>(&mut self, next_id: u32, blocked_names: I)
    where
        I: IntoIterator<Item = String>,
    {
        self.transformer
            .set_disposable_env_context(next_id, blocked_names);
    }

    pub const fn disposable_env_counter(&self) -> u32 {
        self.transformer.disposable_env_counter()
    }

    pub fn take_generated_disposable_env_names(&mut self) -> Vec<String> {
        self.transformer.take_generated_disposable_env_names()
    }

    pub fn set_lexical_this(&mut self, capture: bool) {
        self.this_capture_depth = u32::from(capture);
        self.transformer.set_lexical_this_capture(capture);
    }

    pub fn set_use_this_capture(&mut self, capture: bool) {
        self.this_capture_depth = u32::from(capture);
        self.transformer.set_lexical_this_capture(capture);
    }

    /// Set the class name for private field access transformations
    pub fn set_class_name(&mut self, name: &str) {
        self.class_name = Some(name.to_string());
    }

    /// Declare outer names (e.g. a class-expression alias) that must not be
    /// chosen as the `__generator` state variable.
    pub fn set_outer_reserved_for_generator_state(&mut self, names: Vec<String>) {
        self.outer_reserved_for_generator_state = names;
    }

    pub const fn set_source_map_context(&mut self, source_text: &'a str, source_index: u32) {
        self.source_text = Some(source_text);
        self.source_index = source_index;
        self.transformer.set_source_text(source_text);
    }

    /// Enable source-map capture for the downleveled body. Callers set this only
    /// when a source map is being generated, since the inner `IRPrinter` then
    /// scans its output to position each mapping.
    pub const fn set_capture_mappings(&mut self, capture: bool) {
        self.capture_mappings = capture;
    }

    /// Forward this emitter's source-map capture state to an inner `IRPrinter`.
    const fn configure_printer_capture(&self, printer: &mut IRPrinter<'a>) {
        if self.capture_mappings {
            printer.enable_mapping_capture();
            printer.set_source_map_source_index(self.source_index);
        }
    }

    pub fn take_mappings(&mut self) -> Vec<Mapping> {
        std::mem::take(&mut self.mappings)
    }

    /// Check if a function body contains any await expressions
    pub fn body_contains_await(&self, body_idx: NodeIndex) -> bool {
        self.transformer.body_contains_await(body_idx)
    }

    /// Emit a simple async body with no await (inline format)
    /// Returns: "return __generator(this, function (_a) { return [2 /*return*/]; })"
    pub fn emit_simple_generator_body(&mut self, body_idx: NodeIndex) -> String {
        // Use the transformer to build IR, then print it
        let ir = self.transformer.transform_generator_body(body_idx, false);

        let mut printer = IRPrinter::with_arena(self.arena);
        if let Some(text) = self.source_text {
            printer.set_source_text(text);
        }
        printer.set_indent_level(self.indent_level);
        printer.set_tslib_prefix(self.tslib_helpers.prefix());
        printer.set_tslib_import_binding(self.tslib_helpers.binding().to_string());
        printer.set_system_import_meta(self.system_import_meta);
        printer.set_generator_this_arg(self.generator_this_arg.clone());
        printer.emit(&ir);
        printer.take_output()
    }

    /// Emit a generator body with await (switch/case format)
    pub fn emit_generator_body_with_await(&mut self, body_idx: NodeIndex) -> String {
        // Use the transformer to build IR, then print it
        let ir = self.transformer.transform_generator_body(body_idx, true);

        let mut printer = IRPrinter::with_arena(self.arena);
        if let Some(text) = self.source_text {
            printer.set_source_text(text);
        }
        printer.set_indent_level(self.indent_level);
        printer.set_tslib_prefix(self.tslib_helpers.prefix());
        printer.set_tslib_import_binding(self.tslib_helpers.binding().to_string());
        printer.set_system_import_meta(self.system_import_meta);
        printer.set_generator_this_arg(self.generator_this_arg.clone());
        printer.emit(&ir);
        printer.take_output()
    }

    /// Emit a simple async body with no await, returning hoisted var names.
    pub fn emit_simple_generator_body_with_hoisted_vars(
        &mut self,
        body_idx: NodeIndex,
    ) -> (String, Vec<String>) {
        let (body, groups, _) = self.emit_simple_generator_body_with_hoisted_var_groups(body_idx);
        let hoisted = groups.into_iter().flatten().collect();
        (body, hoisted)
    }

    pub fn emit_simple_generator_body_with_hoisted_var_groups(
        &mut self,
        body_idx: NodeIndex,
    ) -> (String, Vec<Vec<String>>, bool) {
        let (body, hoisted, _, needs_lexical_this_capture) =
            self.emit_generator_body_and_hoisted_vars(body_idx, false);
        (body, hoisted, needs_lexical_this_capture)
    }

    /// Emit a generator body with await, returning hoisted var names.
    pub fn emit_generator_body_with_await_and_hoisted_vars(
        &mut self,
        body_idx: NodeIndex,
    ) -> (String, Vec<String>, Vec<String>) {
        let (body, groups, directives, _) =
            self.emit_generator_body_and_hoisted_vars(body_idx, true);
        (body, groups.into_iter().flatten().collect(), directives)
    }

    pub fn emit_generator_body_with_await_and_hoisted_var_groups(
        &mut self,
        body_idx: NodeIndex,
    ) -> (String, Vec<Vec<String>>, Vec<String>, bool) {
        self.emit_generator_body_and_hoisted_vars(body_idx, true)
    }

    fn emit_generator_body_and_hoisted_vars(
        &mut self,
        body_idx: NodeIndex,
        has_await: bool,
    ) -> (String, Vec<Vec<String>>, Vec<String>, bool) {
        self.emit_generator_body_and_hoisted_vars_skipping(body_idx, has_await, &[])
    }

    pub fn emit_generator_body_and_hoisted_vars_skipping(
        &mut self,
        body_idx: NodeIndex,
        has_await: bool,
        skipped_statements: &[NodeIndex],
    ) -> (String, Vec<Vec<String>>, Vec<String>, bool) {
        let mut ir = self.transformer.transform_generator_body_skipping(
            body_idx,
            has_await,
            skipped_statements,
        );
        let directives = Self::extract_and_remove_directive_prologue(&mut ir);
        let hoisted = self.transformer.extract_hoisted_var_groups(&mut ir);
        let needs_lexical_this_capture = ir.contains_captured_this_reference();
        let mut printer = IRPrinter::with_arena(self.arena);
        if let Some(text) = self.source_text {
            printer.set_source_text(text);
        }
        self.configure_printer_capture(&mut printer);
        printer.set_indent_level(self.indent_level);
        printer.set_tslib_prefix(self.tslib_helpers.prefix());
        printer.set_tslib_import_binding(self.tslib_helpers.binding().to_string());
        printer.set_system_import_meta(self.system_import_meta);
        printer.set_generator_this_arg(self.generator_this_arg.clone());
        let hoisted_names: Vec<&str> = hoisted
            .iter()
            .flat_map(|group| group.iter().map(String::as_str))
            .collect();
        let state_name = if self.outer_reserved_for_generator_state.is_empty() {
            IRPrinter::generator_state_name_for_hoisted(&hoisted_names)
        } else {
            let mut combined: Vec<&str> = hoisted_names.clone();
            combined.extend(
                self.outer_reserved_for_generator_state
                    .iter()
                    .map(String::as_str),
            );
            IRPrinter::generator_state_name_for_hoisted(&combined)
        };
        printer.set_generator_state_name(state_name);
        printer.emit(&ir);
        self.mappings.extend(printer.take_mappings());
        (
            printer.take_output(),
            hoisted,
            directives,
            needs_lexical_this_capture,
        )
    }

    /// Build the full `__awaiter` wrapper for an async function body as
    /// `IRNode::AwaiterCall` (with its nested `IRNode::GeneratorBody`) and
    /// print it through [`IRPrinter`], which owns the wrapper text: inline vs
    /// multi-line callback format, directive prologues, hoisted `var` groups,
    /// the `var _this = this;` lexical capture, and generator state naming.
    ///
    /// `force_multiline` mirrors tsc's rule that a multi-line source body
    /// keeps the multi-line callback format even when nothing is hoisted.
    pub fn emit_awaiter_call(
        &mut self,
        body_idx: NodeIndex,
        has_await: bool,
        this_expr: &str,
        promise_constructor: Option<String>,
        force_multiline: bool,
    ) -> String {
        let mut generator_body =
            self.transformer
                .transform_generator_body_skipping(body_idx, has_await, &[]);
        let directives = Self::extract_and_remove_directive_prologue(&mut generator_body);
        let hoisted_var_groups = self
            .transformer
            .extract_hoisted_var_groups(&mut generator_body);
        let needs_lexical_this_capture = generator_body.contains_captured_this_reference();
        let awaiter_call = IRNode::AwaiterCall {
            this_arg: Box::new(IRNode::Raw(this_expr.to_string().into())),
            generator_body: Box::new(generator_body),
            needs_lexical_this_capture,
            hoisted_var_groups,
            promise_constructor,
            multiline_callback: force_multiline,
            directives,
        };

        let mut printer = IRPrinter::with_arena(self.arena);
        if let Some(text) = self.source_text {
            printer.set_source_text(text);
        }
        self.configure_printer_capture(&mut printer);
        printer.set_indent_level(self.indent_level);
        printer.set_tslib_prefix(self.tslib_helpers.prefix());
        printer.set_tslib_import_binding(self.tslib_helpers.binding().to_string());
        printer.set_helper_import_aliases(self.tslib_helpers.aliases().clone());
        printer.set_system_import_meta(self.system_import_meta);
        printer.set_generator_this_arg(self.generator_this_arg.clone());
        printer.set_outer_reserved_for_generator_state(
            self.outer_reserved_for_generator_state.clone(),
        );
        printer.emit(&awaiter_call);
        self.mappings.extend(printer.take_mappings());
        printer.take_output()
    }

    fn extract_and_remove_directive_prologue(generator_body: &mut IRNode) -> Vec<String> {
        let IRNode::GeneratorBody { cases, .. } = generator_body else {
            return Vec::new();
        };
        let Some(first_case) = cases.first_mut() else {
            return Vec::new();
        };

        let mut directives = Vec::new();
        while let Some(IRNode::ExpressionStatement(expr)) = first_case.statements.first() {
            let directive = match expr.as_ref() {
                IRNode::StringLiteral(text) | IRNode::RawStringLiteral(text) => text.to_string(),
                IRNode::Raw(text) => {
                    let Some(text) = Self::raw_string_directive_text(text) else {
                        break;
                    };
                    text
                }
                _ => break,
            };
            directives.push(directive);
            first_case.statements.remove(0);
        }
        directives
    }

    fn raw_string_directive_text(text: &str) -> Option<String> {
        let trimmed = text.trim();
        let bytes = trimmed.as_bytes();
        let quote = bytes.first().copied()?;
        if !matches!(quote, b'\'' | b'"') || bytes.last().copied() != Some(quote) {
            return None;
        }
        Some(trimmed[1..trimmed.len() - 1].to_string())
    }

    /// Emit a complete async function transformation
    pub fn emit_async_function(&mut self, func_idx: NodeIndex) -> String {
        let ir = self.transformer.transform_async_function(func_idx);

        let mut printer = IRPrinter::with_arena(self.arena);
        if let Some(text) = self.source_text {
            printer.set_source_text(text);
        }
        printer.set_indent_level(self.indent_level);
        printer.set_tslib_prefix(self.tslib_helpers.prefix());
        printer.set_tslib_import_binding(self.tslib_helpers.binding().to_string());
        printer.set_system_import_meta(self.system_import_meta);
        printer.emit(&ir);
        printer.take_output()
    }
}
