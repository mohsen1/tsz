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
use rustc_hash::{FxHashMap, FxHashSet};
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

    /// Whether a downlevel optional chain should wrap its lowered ternary
    /// in parentheses. Set by prefix/postfix unary and conditional-condition
    /// emitters so that `o?.b ? 1 : 0` lowers to `(o === null || o === void 0 ? void 0 : o.b) ? 1 : 0`
    /// and `o?.a++` lowers to `(o === null || o === void 0 ? void 0 : o.a)++`.
    pub optional_chain_needs_parens: bool,

    /// Whether a downlevel nullish coalescing expression should wrap its lowered
    /// ternary in parentheses. Set in the same contexts as `optional_chain_needs_parens`.
    /// e.g., `a ?? b || c` → `(a !== null && a !== void 0 ? a : b) || c`
    pub nullish_coalescing_needs_parens: bool,

    /// Whether the leftmost function/object expression should self-parenthesize.
    /// Set by `emit_expression_statement` when the statement is a call expression
    /// whose direct callee is a function/object expression. This produces TSC-style
    /// `(function(){})()` instead of `(function(){}())`.
    pub paren_leftmost_function_or_object: bool,

    /// Whether the current expression's result value is discarded (statement context).
    /// Set by expression-statement and for-loop incrementor emitters so that
    /// postfix unary lowering can use the simpler (non-value-preserving) form.
    pub in_statement_expression: bool,
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
        crate::transforms::emit_utils::next_temp_var_name(&mut self.temp_var_counter)
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

    /// Function exports that were hoisted to the preamble
    /// (`exports.f = f;` emitted before any statements). Used to skip
    /// duplicate inline emission in `export { f }` clauses.
    /// Each entry is `(exported_name, local_name)`.
    pub hoisted_func_exports: Vec<(String, String)>,

    /// Whether a `export default function func()` was hoisted to the preamble
    /// (`exports.default = func;`). When true, skip inline emission.
    pub default_func_export_hoisted: bool,

    /// Whether the file contains an `export =` assignment (`CommonJS` export assignment)
    /// If true, other named exports should be suppressed in `CommonJS` emit.
    pub has_export_assignment: bool,

    /// Per-base counters for generated CommonJS module bindings.
    /// Keeps numbering stable by module base name (e.g., `foo_1`, `bar_1`, `foo_2`).
    pub module_temp_counters: FxHashMap<String, u32>,

    /// Names whose exports were already folded into a namespace/enum IIFE closing
    /// (e.g., `(A || (exports.A = A = {}))`). Used to suppress duplicate
    /// `exports.A = A;` emission in `export { A }` re-export handling.
    pub iife_exported_names: FxHashSet<String>,

    /// Names whose `exports.X = X;` was already emitted inline after their
    /// declaration. Used to suppress duplicate emission in `export { X }` clauses.
    pub inline_exported_names: FxHashSet<String>,

    /// Exports whose variable declaration was inlined as `exports.x = val;`
    /// (no local `const/let/var x` exists in output). Used to determine
    /// whether `export default x` should emit `exports.default = exports.x;`
    /// (inlined) or `exports.default = x;` (local declaration exists).
    pub inlined_var_exports: FxHashSet<String>,

    /// Names that have runtime value declarations in the file (syntactically
    /// determined). Used to filter `export { I }` specifiers where `I` is an
    /// interface/type-alias/non-instantiated-namespace — these are type-only
    /// and should not produce `exports.I = I;` at runtime.
    pub value_declaration_names: FxHashSet<String>,

    /// Whether `value_declaration_names` has been computed. Distinguishes
    /// "not yet computed" (false) from "computed but empty" (true with empty set).
    pub value_decl_names_computed: bool,

    /// Whether any local `export { ... }` clause was fully elided (all specifiers
    /// were type-only). When true and the file has no other module syntax (no value
    /// exports/imports survived), an `export {};` marker must be emitted at the end
    /// to preserve module semantics.
    pub had_elided_export_clause: bool,
}

impl ModuleTransformState {
    /// Enter `CommonJS` module mode
    pub const fn enter_commonjs(&mut self) {
        self.commonjs_mode = true;
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

    /// Whether exponentiation (`**` / `**=`) needs downleveling (target < ES2016)
    pub needs_es2016_lowering: bool,

    /// Whether ES2018 features need downleveling (object rest/spread → `__rest`).
    pub needs_es2018_lowering: bool,

    /// Whether ES2019 features need downleveling (optional catch binding).
    pub needs_es2019_lowering: bool,

    /// Whether ES2020 features need downleveling (optional chaining/nullish/coalescing).
    pub needs_es2020_lowering: bool,

    /// Whether ES2021 features need downleveling (logical assignment operators).
    pub needs_es2021_lowering: bool,

    /// Whether ES2022 features need downleveling (class fields/private fields/static blocks).
    pub needs_es2022_lowering: bool,

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

    /// When true, `await expr` becomes `yield __await(expr)` for async generator lowering.
    pub emit_await_as_yield_await: bool,

    /// When true, rewrite `arguments` identifiers to `arguments_1` inside async
    /// generator bodies (ES2015+ path). The outer function captures arguments
    /// with `var arguments_1 = arguments;` so the generator closure can reference
    /// the correct scope.
    pub rewrite_arguments_to_arguments_1: bool,

    /// Auto-detect module mode: if true, detect imports/exports and apply `CommonJS`
    pub auto_detect_module: bool,

    /// Original module kind before wrapper body override (AMD/UMD → `CommonJS`).
    /// Used by export assignment to emit `return X` instead of `module.exports = X` in AMD.
    pub original_module_kind: Option<ModuleKind>,
    pub file_is_module: bool,
}

impl EmitContext {
    /// Create a new `EmitContext` with default options
    pub fn new() -> Self {
        Self::with_options(PrinterOptions::default())
    }

    /// Create a new `EmitContext` with the given options
    pub fn with_options(options: PrinterOptions) -> Self {
        let mut ctx = Self {
            options,
            flags: EmitFlags::default(),
            target_es5: false,
            needs_es2016_lowering: false,
            needs_es2018_lowering: false,
            needs_es2019_lowering: false,
            needs_es2020_lowering: false,
            needs_es2021_lowering: false,
            needs_es2022_lowering: false,
            needs_async_lowering: false,
            arrow_state: ArrowTransformState::default(),
            destructuring_state: DestructuringState::default(),
            module_state: ModuleTransformState::default(),
            block_scope_state: BlockScopeState::default(),
            private_field_state: PrivateFieldState::default(),
            emit_await_as_yield: false,
            emit_await_as_yield_await: false,
            rewrite_arguments_to_arguments_1: false,
            auto_detect_module: false,
            original_module_kind: None,
            file_is_module: false,
        };
        ctx.sync_target_gates();
        ctx
    }

    const fn sync_target_gates(&mut self) {
        let target = self.options.target;
        self.target_es5 = matches!(target, ScriptTarget::ES3 | ScriptTarget::ES5);
        self.needs_es2016_lowering = !target.supports_es2016();
        self.needs_es2018_lowering = !target.supports_es2018();
        self.needs_es2019_lowering = !target.supports_es2019();
        self.needs_es2020_lowering = !target.supports_es2020();
        self.needs_es2021_lowering = !target.supports_es2021();
        self.needs_es2022_lowering = !target.supports_es2022();
        self.needs_async_lowering = !target.supports_es2017();
    }

    /// Set the full script target and refresh all derived target gates.
    pub const fn set_target(&mut self, target: ScriptTarget) {
        self.options.target = target;
        self.sync_target_gates();
    }

    /// Set whether we are targeting ES5-like output.
    ///
    /// Keeps `target_es5`, `options.target`, and feature gates in sync.
    pub const fn set_target_es5(&mut self, es5: bool) {
        self.set_target(if es5 {
            ScriptTarget::ES5
        } else {
            ScriptTarget::ES2015
        })
    }

    /// Create an `EmitContext` targeting ES5
    pub fn es5() -> Self {
        let mut ctx = Self::new();
        ctx.set_target_es5(true);
        ctx
    }

    /// Create an `EmitContext` targeting ES6+
    pub fn es6() -> Self {
        let mut ctx = Self::new();
        ctx.set_target_es5(false);
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
        self.options.module.is_commonjs()
    }

    /// Check if we're effectively in `CommonJS` mode, even when the module kind
    /// is temporarily set to `None` inside export body emission.
    ///
    /// During CJS export emission, `options.module` is temporarily set to `None`
    /// to prevent re-applying CJS transforms. But JSX calls still need to know
    /// the true module kind to emit `(0, jsx_runtime_1.jsx)()` vs `_jsx()`.
    pub const fn is_effectively_commonjs(&self) -> bool {
        if self.options.module.is_commonjs() {
            return true;
        }
        if let Some(original) = self.original_module_kind {
            return original.is_commonjs();
        }
        false
    }
}

impl Default for EmitContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "../../tests/emit_context.rs"]
mod tests;
