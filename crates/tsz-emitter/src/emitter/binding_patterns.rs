//! Binding Pattern Emission Module
//!
//! This module handles emission of destructuring binding patterns.
//! Includes object binding patterns, array binding patterns, and binding elements.

use super::Printer;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> Printer<'a> {
    // =========================================================================
    // Binding Patterns
    // =========================================================================

    /// Emit an object binding pattern: { x, y }
    pub(super) fn emit_object_binding_pattern(&mut self, node: &Node) {
        let Some(pattern) = self.arena.get_binding_pattern(node) else {
            return;
        };

        if pattern.elements.nodes.is_empty() {
            self.write("{}");
            return;
        }

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
            // Check for shorthand: { name } where property_name text == name text
            if self.is_shorthand_binding(elem.property_name, elem.name) {
                self.emit(elem.name);
            } else {
                self.emit(elem.property_name);
                self.write(": ");
                self.emit(elem.name);
            }
        } else {
            self.emit(elem.name);
        }

        // Default value: = expr
        if !elem.initializer.is_none() {
            self.write(" = ");
            self.emit(elem.initializer);
        }
    }

    // =========================================================================
    // Binding Pattern Utilities
    // =========================================================================

    /// Get the next temporary variable name.
    /// Uses the unified `make_unique_name` to ensure collision-free names
    /// across both destructuring and for-of lowering.
    pub(super) fn get_temp_var_name(&mut self) -> String {
        self.make_unique_name()
    }

    /// Check if a binding element is a shorthand (property_name text == name text)
    fn is_shorthand_binding(&self, property_name: NodeIndex, name: NodeIndex) -> bool {
        let prop_text = self
            .arena
            .get(property_name)
            .and_then(|n| self.arena.get_identifier(n))
            .map(|id| id.escaped_text.as_str());
        let name_text = self
            .arena
            .get(name)
            .and_then(|n| self.arena.get_identifier(n))
            .map(|id| id.escaped_text.as_str());
        match (prop_text, name_text) {
            (Some(p), Some(n)) => p == n,
            _ => false,
        }
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
