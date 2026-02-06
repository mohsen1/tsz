//! Binding Pattern Emission Module
//!
//! This module handles emission of destructuring binding patterns.
//! Includes object binding patterns, array binding patterns, and binding elements.

use super::Printer;
use crate::parser::NodeIndex;
use crate::parser::node::Node;
use crate::parser::syntax_kind_ext;

impl<'a> Printer<'a> {
    // =========================================================================
    // Binding Patterns
    // =========================================================================

    /// Emit an object binding pattern: { x, y }
    pub(super) fn emit_object_binding_pattern(&mut self, node: &Node) {
        let Some(pattern) = self.arena.get_binding_pattern(node) else {
            return;
        };

        self.write("{ ");
        self.emit_comma_separated(&pattern.elements.nodes);
        self.write(" }");
    }

    /// Emit an array binding pattern: [x, y]
    pub(super) fn emit_array_binding_pattern(&mut self, node: &Node) {
        let Some(pattern) = self.arena.get_binding_pattern(node) else {
            return;
        };

        self.write("[");
        self.emit_comma_separated(&pattern.elements.nodes);
        self.write("]");
    }

    /// Emit a binding element: x or x = default or propertyName: x
    pub(super) fn emit_binding_element(&mut self, node: &Node) {
        let Some(elem) = self.arena.get_binding_element(node) else {
            return;
        };

        // Rest element: ...x
        if elem.dot_dot_dot_token {
            self.write("...");
        }

        // propertyName: name  or just name
        if !elem.property_name.is_none() {
            self.emit(elem.property_name);
            self.write(": ");
        }

        self.emit(elem.name);

        // Default value: = expr
        if !elem.initializer.is_none() {
            self.write(" = ");
            self.emit(elem.initializer);
        }
    }

    // =========================================================================
    // Binding Pattern Utilities
    // =========================================================================

    /// Get the next temporary variable name
    /// Pattern: _a, _b, ..., _z, _a_2, _b_2, ..., _z_2, _a_3, ...
    /// This avoids collisions (the old % 26 approach would repeat _a after _z)
    pub(super) fn get_temp_var_name(&mut self) -> String {
        let counter = self.ctx.destructuring_state.temp_var_counter;
        let letter = (b'a' + (counter % 26) as u8) as char;
        let suffix = (counter / 26) + 1;

        let name = if suffix == 1 {
            format!("_{}", letter)
        } else {
            format!("_{}_{}", letter, suffix)
        };

        self.ctx.destructuring_state.temp_var_counter += 1;
        name
    }

    /// Check if a node is a binding pattern
    pub(super) fn is_binding_pattern(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
            || node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
    }
}
