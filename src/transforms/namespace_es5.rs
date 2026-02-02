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

use crate::parser::NodeIndex;
use crate::parser::node::NodeArena;
use crate::transforms::ir_printer::IRPrinter;
use crate::transforms::namespace_es5_ir::NamespaceES5Transformer;

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
    transformer: NamespaceES5Transformer<'a>,
}

impl<'a> NamespaceES5Emitter<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        NamespaceES5Emitter {
            arena,
            source_text: None,
            indent_level: 0,
            transformer: NamespaceES5Transformer::new(arena),
        }
    }

    /// Create a namespace emitter with CommonJS mode
    pub fn with_commonjs(arena: &'a NodeArena, is_commonjs: bool) -> Self {
        NamespaceES5Emitter {
            arena,
            source_text: None,
            indent_level: 0,
            transformer: NamespaceES5Transformer::with_commonjs(arena, is_commonjs),
        }
    }

    /// Set the source text for ASTRef emission
    pub fn set_source_text(&mut self, text: &'a str) {
        self.source_text = Some(text);
    }

    /// Emit a namespace declaration
    pub fn emit_namespace(&mut self, ns_idx: NodeIndex) -> String {
        let ir = self.transformer.transform_namespace(ns_idx);
        let ir = match ir {
            Some(ir) => ir,
            None => return String::new(),
        };

        let mut printer = IRPrinter::with_arena(self.arena);
        printer.set_indent_level(self.indent_level);
        if let Some(source_text) = self.source_text {
            printer.set_source_text(source_text);
        }
        printer.emit(&ir).to_string()
    }

    /// Set the indent level for output
    pub fn set_indent_level(&mut self, level: u32) {
        self.indent_level = level;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ParserState;

    fn emit_namespace(source: &str) -> String {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        // Find the namespace declaration
        if let Some(root_node) = parser.arena.get(root)
            && let Some(source_file) = parser.arena.get_source_file(root_node)
            && let Some(&ns_idx) = source_file.statements.nodes.first()
        {
            let mut emitter = NamespaceES5Emitter::new(&parser.arena);
            emitter.set_source_text(source);
            return emitter.emit_namespace(ns_idx);
        }
        String::new()
    }

    #[test]
    fn test_empty_namespace_skipped() {
        let output = emit_namespace("namespace M { }");
        assert!(
            output.is_empty() || output.trim().is_empty(),
            "Empty namespace should produce no output"
        );
    }

    #[test]
    fn test_namespace_with_content() {
        let output = emit_namespace("namespace M { export var x = 1; }");
        assert!(output.contains("var M;"), "Should declare var M");
        assert!(output.contains("(function (M)"), "Should have IIFE");
        assert!(
            output.contains("(M || (M = {}))"),
            "Should have M || (M = {{}})"
        );
    }

    #[test]
    fn test_namespace_with_function() {
        let output = emit_namespace("namespace M { export function foo() { return 1; } }");
        eprintln!("=== Namespace output ===");
        eprintln!("{}", output);
        eprintln!("=== End output ===");
        assert!(output.contains("var M;"), "Should declare var M");
        assert!(
            output.contains("function foo()"),
            "Should have function foo"
        );
        assert!(output.contains("M.foo = foo;"), "Should export foo");
    }

    // Note: test_declare_namespace_skipped is skipped because the parser
    // currently doesn't attach the `declare` modifier to namespace nodes.
    // This is a known parser limitation that should be fixed separately.
    // The has_declare_modifier() check is still in place for when the parser is fixed.
}
