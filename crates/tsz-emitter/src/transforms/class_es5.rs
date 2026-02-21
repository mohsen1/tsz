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
    /// Transform directives for `ASTRef` nodes
    transforms: Option<TransformContext>,
}

impl<'a> ClassES5Emitter<'a> {
    pub const fn new(arena: &'a NodeArena) -> Self {
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

    /// Set transform directives for `ASTRef` nodes
    pub fn set_transforms(&mut self, transforms: TransformContext) {
        self.transforms = Some(transforms.clone());
        // Also pass transforms to ES5ClassTransformer for directive-aware conversion
        self.transformer.set_transforms(transforms);
    }

    /// Set the initial indentation level (to match the parent context)
    pub const fn set_indent_level(&mut self, level: u32) {
        self.indent_level = level;
    }

    /// Set the source text (for `ASTRef` emission)
    pub const fn set_source_text(&mut self, source_text: &'a str) {
        self.source_text = Some(source_text);
        self.transformer.set_source_text(source_text);
    }

    /// Set source map context
    ///
    /// Note: Source maps are not currently supported in the IR-based approach.
    /// This method is kept for API compatibility.
    pub const fn set_source_map_context(&mut self, source_text: &'a str, source_index: u32) {
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
            if i + 3 > bytes.len() || &bytes[i..i + 3] != b"var" {
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
            let ident = String::from_utf8_lossy(&bytes[ident_start..i]).to_string();
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
#[path = "../../tests/class_es5.rs"]
mod tests;
