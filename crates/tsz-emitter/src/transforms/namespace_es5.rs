//! ES5 Namespace Transform
//!
//! Transforms TypeScript namespaces to ES5 IIFE patterns:
//!
//! ```typescript
//! namespace foo {
//!     export class Provide { }
//! }
//! ```
//!
//! Becomes:
//!
//! ```javascript
//! var foo;
//! (function (foo) {
//!     var Provide = /** @class */ (function () {
//!         function Provide() { }
//!         return Provide;
//!     }());
//!     foo.Provide = Provide;
//! })(foo || (foo = {}));
//! ```
//!
//! Also handles qualified names like `namespace A.B.C`:
//! ```javascript
//! var A;
//! (function (A) {
//!     var B;
//!     (function (B) {
//!         var C;
//!         (function (C) {
//!             // body
//!         })(C = B.C || (B.C = {}));
//!     })(B = A.B || (A.B = {}));
//! })(A || (A = {}));
//! ```

use crate::transforms::ir_printer::IRPrinter;
use crate::transforms::namespace_es5_ir::NamespaceES5Transformer;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;

/// Namespace ES5 emitter
///
/// This is a thin wrapper around `NamespaceES5Transformer` and `IRPrinter`
/// for backward compatibility.
///
/// # Architecture
///
/// - Uses `NamespaceES5Transformer` to produce IR nodes
/// - Uses `IRPrinter` to emit IR nodes as JavaScript strings
/// - Maintains the same public API as the original implementation
pub struct NamespaceES5Emitter<'a> {
    arena: &'a NodeArena,
    source_text: Option<&'a str>,
    indent_level: u32,
    should_declare_var: bool,
    transformer: NamespaceES5Transformer<'a>,
}

impl<'a> NamespaceES5Emitter<'a> {
    pub const fn new(arena: &'a NodeArena) -> Self {
        NamespaceES5Emitter {
            arena,
            source_text: None,
            indent_level: 0,
            should_declare_var: true, // Default to true for backward compatibility
            transformer: NamespaceES5Transformer::new(arena),
        }
    }

    /// Create a namespace emitter with `CommonJS` mode
    pub const fn with_commonjs(arena: &'a NodeArena, is_commonjs: bool) -> Self {
        NamespaceES5Emitter {
            arena,
            source_text: None,
            indent_level: 0,
            should_declare_var: true, // Default to true for backward compatibility
            transformer: NamespaceES5Transformer::with_commonjs(arena, is_commonjs),
        }
    }

    /// Set the source text for `ASTRef` emission and comment extraction
    pub fn set_source_text(&mut self, text: &'a str) {
        self.source_text = Some(text);
        self.transformer.set_source_text(text);
    }

    /// Set whether to emit a 'var' declaration for the namespace
    /// When false (e.g., when merging with a class/enum/function), the 'var' is omitted
    pub const fn set_should_declare_var(&mut self, value: bool) {
        self.should_declare_var = value;
    }

    /// Emit a namespace declaration
    pub fn emit_namespace(&mut self, ns_idx: NodeIndex) -> String {
        let ir = self
            .transformer
            .transform_namespace_with_var_flag(ns_idx, self.should_declare_var);
        let ir = match ir {
            Some(ir) => ir,
            None => return String::new(),
        };

        let mut printer = if let Some(source_text) = self.source_text {
            IRPrinter::with_arena_and_source(self.arena, source_text)
        } else {
            IRPrinter::with_arena(self.arena)
        };
        printer.set_indent_level(self.indent_level);
        printer.emit(&ir).to_string()
    }

    /// Emit an exported namespace declaration (`CommonJS` attach-to-exports form).
    pub fn emit_exported_namespace(&mut self, ns_idx: NodeIndex) -> String {
        let ir = self.transformer.transform_exported_namespace(ns_idx);
        let ir = match ir {
            Some(ir) => ir,
            None => return String::new(),
        };

        let mut printer = if let Some(source_text) = self.source_text {
            IRPrinter::with_arena_and_source(self.arena, source_text)
        } else {
            IRPrinter::with_arena(self.arena)
        };
        printer.set_indent_level(self.indent_level);
        printer.emit(&ir).to_string()
    }

    /// Set the indent level for output
    pub const fn set_indent_level(&mut self, level: u32) {
        self.indent_level = level;
    }
}

#[cfg(test)]
#[path = "../../tests/namespace_es5.rs"]
mod tests;
