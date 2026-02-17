//! `EmitContext` - Transform state management for the emitter
//!
//! This module extracts transform-specific state from Printer into a dedicated
//! context object. This follows the "Transform Context" pattern to:
//!
//! 1. Keep Printer focused on AST traversal
//! 2. Group related state together
//! 3. Make transform state explicit and easier to pass around
//! 4. Enable transforms to manage their own state without bloating Printer

use crate::emitter::PrinterOptions;
use crate::transforms::block_scoping_es5::BlockScopeState;
use crate::transforms::private_fields_es5::PrivateFieldState;
use tsz_common::common::{ModuleKind, NewLineKind, ScriptTarget};

/// Flags that control emission behavior for the current scope/branch
#[derive(Debug, Clone, Default)]
pub struct EmitFlags {
    /// Whether we're inside an async function (for await transforms)
    pub in_async: bool,

    /// Whether we're inside a generator function
    pub in_generator: bool,

    /// Whether to capture `this` as `_this` for arrow functions
    pub capture_this: bool,

    /// Whether we're inside a computed property name
    pub in_computed_property_name: bool,

    /// Whether we're inside a class static block
    pub in_class_static_block: bool,

    /// Whether we're emitting for declaration files (.d.ts)
    pub in_declaration_emit: bool,

    /// Whether we're inside a binary/conditional expression operand.
    /// Used to wrap yield-from-await in parens for correct precedence.
    pub in_binary_operand: bool,
}

impl EmitFlags {
    /// Create default flags
    pub fn new() -> Self {
        Self::default()
    }

    /// Create flags for an async context
    pub fn async_context() -> Self {
        Self {
            in_async: true,
            ..Default::default()
        }
    }

    /// Create flags for a generator context
    pub fn generator_context() -> Self {
        Self {
            in_generator: true,
            ..Default::default()
        }
    }
}

/// State for arrow function ES5 transformation
#[derive(Debug, Default)]
pub struct ArrowTransformState {
    /// Depth of arrow functions that need `_this` capture
    /// When > 0, emit `_this` instead of `this` inside arrow function bodies
    pub this_capture_depth: u32,

    /// Whether we've already emitted `var _this = this;` in the current scope
    pub this_captured_in_scope: bool,
}

impl ArrowTransformState {
    /// Enter an arrow function that uses `this`
    pub const fn enter_arrow_with_this(&mut self) {
        self.this_capture_depth += 1;
    }

    /// Exit an arrow function that uses `this`
    pub const fn exit_arrow_with_this(&mut self) {
        if self.this_capture_depth > 0 {
            self.this_capture_depth -= 1;
        }
    }

    /// Check if we're currently capturing `this`
    pub const fn is_capturing_this(&self) -> bool {
        self.this_capture_depth > 0
    }

    /// Mark that `var _this = this;` has been emitted
    pub const fn mark_this_captured(&mut self) {
        self.this_captured_in_scope = true;
    }

    /// Check if `_this` capture statement has been emitted
    pub const fn is_this_captured(&self) -> bool {
        self.this_captured_in_scope
    }

    /// Reset for a new scope (entering a function/class)
    pub const fn enter_new_scope(&mut self) {
        self.this_captured_in_scope = false;
    }
}

/// State for destructuring transformation
#[derive(Debug, Default)]
pub struct DestructuringState {
    /// Counter for temporary variables (_a, _b, _c, etc.)
    pub temp_var_counter: u32,
    /// Counter for for-of loop temp variables (_i/_a, _b/_c, _d/_e, etc.)
    pub for_of_counter: u32,
}

impl DestructuringState {
    /// Get the next temporary variable name
    pub fn next_temp_var(&mut self) -> String {
        let name = format!("_{}", (b'a' + (self.temp_var_counter % 26) as u8) as char);
        self.temp_var_counter += 1;
        name
    }

    /// Reset the counter (for a new file)
    pub const fn reset(&mut self) {
        self.temp_var_counter = 0;
        self.for_of_counter = 0;
    }
}

/// State for `CommonJS` module transformation
#[derive(Debug, Default)]
pub struct ModuleTransformState {
    /// Whether we're currently inside `CommonJS` module transformation
    pub commonjs_mode: bool,

    /// Collected exported names for `CommonJS` (to emit exports.X = X; after declarations)
    pub pending_exports: Vec<String>,

    /// Whether "use strict" has been emitted
    pub strict_mode_emitted: bool,

    /// Whether the file contains an `export =` assignment (`CommonJS` export assignment)
    /// If true, other named exports should be suppressed in `CommonJS` emit.
    pub has_export_assignment: bool,
}

impl ModuleTransformState {
    /// Enter `CommonJS` module mode
    pub const fn enter_commonjs(&mut self) {
        self.commonjs_mode = true;
    }

    /// Exit `CommonJS` module mode
    pub fn exit_commonjs(&mut self) {
        self.commonjs_mode = false;
        self.pending_exports.clear();
    }

    /// Add an export name
    pub fn add_export(&mut self, name: String) {
        self.pending_exports.push(name);
    }

    /// Take and clear the pending exports
    pub fn take_exports(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending_exports)
    }
}

/// The main emit context that holds all transform-specific state
///
/// This is passed through the emitter and transforms, allowing them to
/// access and modify state without bloating Printer.
#[derive(Debug)]
pub struct EmitContext {
    /// Printer/emitter options
    pub options: PrinterOptions,

    /// Current emit flags for this scope
    pub flags: EmitFlags,

    /// Whether to emit ES5 (classes→IIFEs, arrows→functions)
    pub target_es5: bool,

    /// Whether async/await needs lowering (target < ES2017)
    /// ES2015/ES2016 use __awaiter + generators (yield)
    /// ES5 additionally lowers generators to state machines (__generator)
    pub needs_async_lowering: bool,

    /// Arrow function transformation state
    pub arrow_state: ArrowTransformState,

    /// Destructuring transformation state
    pub destructuring_state: DestructuringState,

    /// Module transformation state
    pub module_state: ModuleTransformState,

    /// Block scoping transformation state (let/const → var)
    pub block_scope_state: BlockScopeState,

    /// Private fields transformation state (#field → `WeakMap`)
    pub private_field_state: PrivateFieldState,

    /// When true, emit `yield` instead of `await` in expression positions.
    /// Used for ES2015/ES2016 async lowering (function* + yield pattern).
    pub emit_await_as_yield: bool,

    /// Auto-detect module mode: if true, detect imports/exports and apply `CommonJS`
    pub auto_detect_module: bool,

    /// Original module kind before wrapper body override (AMD/UMD → `CommonJS`).
    /// Used by export assignment to emit `return X` instead of `module.exports = X` in AMD.
    pub original_module_kind: Option<ModuleKind>,
}

impl EmitContext {
    /// Create a new `EmitContext` with default options
    pub fn new() -> Self {
        Self::with_options(PrinterOptions::default())
    }

    /// Create a new `EmitContext` with the given options
    pub fn with_options(options: PrinterOptions) -> Self {
        let target_es5 = matches!(options.target, ScriptTarget::ES3 | ScriptTarget::ES5);
        let needs_async_lowering = !options.target.supports_es2017();

        Self {
            options,
            flags: EmitFlags::default(),
            target_es5,
            needs_async_lowering,
            arrow_state: ArrowTransformState::default(),
            destructuring_state: DestructuringState::default(),
            module_state: ModuleTransformState::default(),
            block_scope_state: BlockScopeState::default(),
            private_field_state: PrivateFieldState::default(),
            emit_await_as_yield: false,
            auto_detect_module: false,
            original_module_kind: None,
        }
    }

    /// Create an `EmitContext` targeting ES5
    pub fn es5() -> Self {
        let mut ctx = Self::new();
        ctx.target_es5 = true;
        ctx.needs_async_lowering = true;
        ctx.options.target = ScriptTarget::ES5;
        ctx
    }

    /// Create an `EmitContext` targeting ES6+
    pub fn es6() -> Self {
        let mut ctx = Self::new();
        ctx.target_es5 = false;
        ctx.needs_async_lowering = true; // ES2015 still needs async lowering
        ctx.options.target = ScriptTarget::ES2015;
        ctx
    }

    // =========================================================================
    // Convenience accessors
    // =========================================================================

    /// Check if targeting ES5 (needs class/arrow transforms)
    pub const fn is_es5(&self) -> bool {
        self.target_es5
    }

    /// Get the new line string based on options
    pub const fn new_line(&self) -> &'static str {
        match self.options.new_line {
            NewLineKind::LineFeed => "\n",
            NewLineKind::CarriageReturnLineFeed => "\r\n",
        }
    }

    /// Check if comments should be removed
    pub const fn remove_comments(&self) -> bool {
        self.options.remove_comments
    }

    /// Check if we're in `CommonJS` mode
    pub const fn is_commonjs(&self) -> bool {
        matches!(self.options.module, ModuleKind::CommonJS)
    }

    // =========================================================================
    // Scope management helpers
    // =========================================================================

    /// Enter a new function scope (resets certain state)
    pub const fn enter_function_scope(&mut self) {
        self.arrow_state.enter_new_scope();
    }

    /// Enter a class scope
    pub const fn enter_class_scope(&mut self) {
        self.arrow_state.enter_new_scope();
    }

    // =========================================================================
    // Temp variable generation
    // =========================================================================

    /// Get the next temporary variable name for destructuring
    pub fn next_temp_var(&mut self) -> String {
        self.destructuring_state.next_temp_var()
    }

    /// Get the current temp var counter value
    pub const fn temp_var_counter(&self) -> u32 {
        self.destructuring_state.temp_var_counter
    }

    /// Set the temp var counter (for restoring state)
    pub const fn set_temp_var_counter(&mut self, value: u32) {
        self.destructuring_state.temp_var_counter = value;
    }
}

impl Default for EmitContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "../tests/emit_context.rs"]
mod tests;
