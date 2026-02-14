//! ES5 Class Transform
//!
//! Transforms ES6 classes to ES5 IIFE patterns:
//!
//! ```typescript
//! class Animal {
//!     constructor(name) { this.name = name; }
//!     speak() { console.log(this.name); }
//! }
//! ```
//!
//! Becomes:
//!
//! ```javascript
//! var Animal = /** @class */ (function () {
//!     function Animal(name) {
//!         this.name = name;
//!     }
//!     Animal.prototype.speak = function () {
//!         console.log(this.name);
//!     };
//!     return Animal;
//! }());
//! ```
//!
//! # Architecture
//!
//! This module is a thin wrapper around `ES5ClassTransformer` and `IRPrinter`.
//!
//! - Uses `ES5ClassTransformer` from `class_es5_ir` to produce IR nodes
//! - Uses `IRPrinter` to emit IR nodes as JavaScript strings
//! - Maintains the same public API as the original implementation for backward compatibility

use crate::transform_context::TransformContext;
use crate::transforms::class_es5_ir::ES5ClassTransformer;
use crate::transforms::ir_printer::IRPrinter;
use tsz_common::source_map::Mapping;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;

/// ES5 class emitter - emits ES5 IIFE pattern for classes
///
/// This is a thin wrapper around `ES5ClassTransformer` and `IRPrinter`
/// for backward compatibility.
///
/// # Architecture
///
/// - Uses `ES5ClassTransformer` to produce IR nodes
/// - Uses `IRPrinter` to emit IR nodes as JavaScript strings
/// - Maintains the same public API as the original implementation
pub struct ClassES5Emitter<'a> {
    arena: &'a NodeArena,
    source_text: Option<&'a str>,
    indent_level: u32,
    /// Source index for source maps (currently unused in IR-based approach)
    source_index: u32,
    /// Mappings for source maps (currently empty in IR-based approach)
    mappings: Vec<Mapping>,
    transformer: ES5ClassTransformer<'a>,
    /// Transform directives for ASTRef nodes
    transforms: Option<TransformContext>,
}

impl<'a> ClassES5Emitter<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        ClassES5Emitter {
            arena,
            source_text: None,
            indent_level: 0,
            source_index: 0,
            mappings: Vec::new(),
            transformer: ES5ClassTransformer::new(arena),
            transforms: None,
        }
    }

    /// Set transform directives for ASTRef nodes
    pub fn set_transforms(&mut self, transforms: TransformContext) {
        self.transforms = Some(transforms.clone());
        // Also pass transforms to ES5ClassTransformer for directive-aware conversion
        self.transformer.set_transforms(transforms);
    }

    /// Set the initial indentation level (to match the parent context)
    pub fn set_indent_level(&mut self, level: u32) {
        self.indent_level = level;
    }

    /// Set the source text (for ASTRef emission)
    pub fn set_source_text(&mut self, source_text: &'a str) {
        self.source_text = Some(source_text);
        self.transformer.set_source_text(source_text);
    }

    /// Set source map context
    ///
    /// Note: Source maps are not currently supported in the IR-based approach.
    /// This method is kept for API compatibility.
    pub fn set_source_map_context(&mut self, source_text: &'a str, source_index: u32) {
        self.source_text = Some(source_text);
        self.source_index = source_index;
        self.transformer.set_source_text(source_text);
    }

    /// Take accumulated source mappings
    ///
    /// Note: Source maps are not currently supported in the IR-based approach.
    /// Returns an empty vector for API compatibility.
    pub fn take_mappings(&mut self) -> Vec<Mapping> {
        std::mem::take(&mut self.mappings)
    }

    /// Emit a class declaration to ES5
    pub fn emit_class(&mut self, class_idx: NodeIndex) -> String {
        self.emit_class_internal(class_idx, None)
    }

    /// Emit a class declaration to ES5 with an override name
    pub fn emit_class_with_name(&mut self, class_idx: NodeIndex, name: &str) -> String {
        self.emit_class_internal(class_idx, Some(name))
    }

    fn emit_class_internal(&mut self, class_idx: NodeIndex, override_name: Option<&str>) -> String {
        let ir = if let Some(name) = override_name {
            self.transformer
                .transform_class_to_ir_with_name(class_idx, Some(name))
        } else {
            self.transformer.transform_class_to_ir(class_idx)
        };

        let ir = match ir {
            Some(ir) => ir,
            None => return String::new(),
        };

        let mut printer = IRPrinter::with_arena(self.arena);
        printer.set_indent_level(self.indent_level);
        if let Some(source_text) = self.source_text {
            printer.set_source_text(source_text);
        }
        if let Some(ref transforms) = self.transforms {
            printer.set_transforms(transforms.clone());
        }
        let mut output = printer.emit(&ir).to_string();
        if let Some(recovery_emit) = self.emit_var_function_recovery(class_idx) {
            output.push('\n');
            output.push_str(&recovery_emit);
        }
        output
    }

    /// TypeScript parser recovery parity for malformed class members like:
    /// `var constructor() { }`
    /// which tsc emits as:
    /// `var constructor;`
    /// `(function () { });`
    fn emit_var_function_recovery(&self, class_idx: NodeIndex) -> Option<String> {
        let text = self.source_text?;
        let class_node = self.arena.get(class_idx)?;
        let start = std::cmp::min(class_node.pos as usize, text.len());
        let end = std::cmp::min(class_node.end as usize, text.len());
        if start >= end {
            return None;
        }

        let slice = &text[start..end];
        let mut i = 0usize;
        let bytes = slice.as_bytes();

        while i < bytes.len() {
            // Find "var"
            if bytes[i].is_ascii_whitespace() {
                i += 1;
                continue;
            }
            if i + 3 > bytes.len() || &slice[i..i + 3] != "var" {
                i += 1;
                continue;
            }
            i += 3;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            let ident_start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'$')
            {
                i += 1;
            }
            if ident_start == i {
                continue;
            }
            let ident = &slice[ident_start..i];
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] != b'(' {
                continue;
            }
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] != b')' {
                continue;
            }
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] != b'{' {
                continue;
            }
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] != b'}' {
                continue;
            }

            return Some(format!("var {ident};\n(function () {{ }});"));
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_parser::parser::ParserState;
    use tsz_parser::parser::syntax_kind_ext;

    fn emit_class(source: &str) -> String {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        if let Some(root_node) = parser.arena.get(root)
            && let Some(source_file) = parser.arena.get_source_file(root_node)
        {
            for &stmt_idx in &source_file.statements.nodes {
                if let Some(node) = parser.arena.get(stmt_idx)
                    && node.kind == syntax_kind_ext::CLASS_DECLARATION
                {
                    let mut emitter = ClassES5Emitter::new(&parser.arena);
                    emitter.set_source_text(source);
                    return emitter.emit_class(stmt_idx);
                }
            }
        }
        String::new()
    }

    #[test]
    fn test_simple_class() {
        let output = emit_class("class Point { }");
        assert!(
            output.contains("var Point = /** @class */ (function ()"),
            "Should have class IIFE: {}",
            output
        );
        assert!(
            output.contains("function Point()"),
            "Should have constructor: {}",
            output
        );
        assert!(
            output.contains("return Point;"),
            "Should return class name: {}",
            output
        );
    }

    #[test]
    fn test_class_with_constructor() {
        let output = emit_class(
            r#"class Point {
            constructor(x, y) {
                this.x = x;
                this.y = y;
            }
        }"#,
        );
        assert!(
            output.contains("function Point(x, y)"),
            "Should have constructor with params: {}",
            output
        );
    }

    #[test]
    fn test_class_with_extends() {
        let output = emit_class(
            r#"class Dog extends Animal {
            constructor(name) {
                super(name);
            }
        }"#,
        );
        assert!(
            output.contains("(function (_super)"),
            "Should have _super parameter: {}",
            output
        );
        assert!(
            output.contains("__extends(Dog, _super)"),
            "Should have extends helper: {}",
            output
        );
        assert!(
            output.contains("_super.call(this"),
            "Should have super.call pattern: {}",
            output
        );
    }

    #[test]
    fn test_class_with_method() {
        let output = emit_class(
            r#"class Greeter {
            greet() {
                console.log("Hello");
            }
        }"#,
        );
        assert!(
            output.contains("Greeter.prototype.greet = function ()"),
            "Should have prototype method: {}",
            output
        );
    }

    #[test]
    fn test_class_with_static_method() {
        let output = emit_class(
            r#"class Counter {
            static count() {
                return 0;
            }
        }"#,
        );
        assert!(
            output.contains("Counter.count = function ()"),
            "Should have static method: {}",
            output
        );
    }

    #[test]
    fn test_class_with_private_field() {
        let output = emit_class(
            r#"class Container {
            #value = 42;
        }"#,
        );
        assert!(
            output.contains("var _Container_value"),
            "Should have WeakMap declaration: {}",
            output
        );
        assert!(
            output.contains("_Container_value.set("),
            "Should have WeakMap.set call: {}",
            output
        );
    }

    #[test]
    fn test_class_with_getter_setter() {
        let output = emit_class(
            r#"class Person {
            _name: string = "";
            get name() { return this._name; }
            set name(value: string) { this._name = value; }
        }"#,
        );
        assert!(
            output.contains("Object.defineProperty"),
            "Should have Object.defineProperty: {}",
            output
        );
        assert!(output.contains("get:"), "Should have getter: {}", output);
        assert!(output.contains("set:"), "Should have setter: {}", output);
    }

    #[test]
    fn test_declare_class_ignored() {
        let output = emit_class(
            r#"declare class Foo {
            bar(): void;
        }"#,
        );
        assert!(output.is_empty(), "Declare class should produce no output");
    }

    #[test]
    fn test_constructor_trailing_comment_preserved() {
        let output = emit_class(
            r#"class C1 {
            constructor(p3) {
                this.p3 = p3;
            } // OK
        }"#,
        );
        assert!(
            output.contains("} // OK"),
            "Constructor trailing comment should be preserved: {}",
            output
        );
    }

    #[test]
    fn test_var_function_recovery_supports_dollar_identifier() {
        let output = emit_class(
            r#"class C {
            var $constructor() { }
        }"#,
        );
        assert!(
            output.contains("var $constructor;"),
            "Recovery emit should keep `$` in identifier: {}",
            output
        );
    }
}
