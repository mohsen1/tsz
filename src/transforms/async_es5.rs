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
//! - `AsyncES5Transformer` (in async_es5_ir.rs) produces IR nodes
//! - `AsyncES5Emitter` is a thin wrapper that uses `IRPrinter` to emit JavaScript
//!
//! This separation allows clean transform logic while delegating string emission
//! to the centralized `IRPrinter`.

use crate::parser::NodeIndex;
use crate::parser::node::NodeArena;
use crate::source_map::Mapping;
use crate::source_writer::source_position_from_offset;
use crate::transforms::async_es5_ir::AsyncES5Transformer;
use crate::transforms::ir_printer::IRPrinter;

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
    #[allow(dead_code)]
    line: u32,
    #[allow(dead_code)]
    column: u32,
    this_capture_depth: u32,
    class_name: Option<String>,
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
            line: 0,
            column: 0,
            this_capture_depth: 0,
            class_name: None,
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

    #[allow(dead_code)]
    fn record_mapping_for_node(&mut self, node_idx: NodeIndex) {
        let Some(text) = self.source_text else {
            return;
        };

        let Some(node) = self.arena.get(node_idx) else {
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
        printer.emit(&ir);
        printer.take_output()
    }

    /// Emit a complete async function transformation
    pub fn emit_async_function(&mut self, func_idx: NodeIndex) -> String {
        let ir = self.transformer.transform_async_function(func_idx);

        let mut printer = IRPrinter::with_arena(self.arena);
        if let Some(text) = self.source_text {
            printer.set_source_text(text);
        }
        printer.set_indent_level(self.indent_level);
        printer.emit(&ir);
        printer.take_output()
    }
}

#[cfg(test)]
#[path = "async_es5_tests.rs"]
mod tests;
