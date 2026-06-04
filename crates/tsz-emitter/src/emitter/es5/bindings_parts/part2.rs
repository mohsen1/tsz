/// Represents a segment of assignment destructuring output.
/// When the right-hand side is a simple identifier, we access properties/elements directly.
/// When complex, we create a temp variable first.
impl<'a> Printer<'a> {
    pub(in crate::emitter) fn emit_single_object_binding_inline_nested_object_node(
        &mut self,
        pattern_node: NodeIndex,
        initializer: NodeIndex,
        key_idx: NodeIndex,
        allow_expression_emit: bool,
    ) -> bool {
        let Some(pattern_ast) = self.arena.get(pattern_node) else {
            return false;
        };
        let Some(pattern) = self.arena.get_binding_pattern(pattern_ast) else {
            return false;
        };
        if pattern.elements.nodes.is_empty() {
            return false;
        }

        let mut child = NodeIndex::NONE;
        let mut non_rest = 0;
        for &elem_idx in &pattern.elements.nodes {
            if elem_idx.is_none() {
                continue;
            }
            let Some(elem_node) = self.arena.get(elem_idx) else {
                return false;
            };
            let Some(elem) = self.arena.get_binding_element(elem_node) else {
                return false;
            };
            if elem.dot_dot_dot_token {
                return false;
            }
            child = elem_idx;
            non_rest += 1;
            if non_rest > 1 {
                return false;
            }
        }
        if child.is_none() {
            return false;
        }

        let Some(child_node) = self.arena.get(child) else {
            return false;
        };
        let Some(child_elem) = self.arena.get_binding_element(child_node) else {
            return false;
        };
        if self.is_binding_pattern(child_elem.name) || !self.has_identifier_text(child_elem.name) {
            return false;
        }

        let child_key_idx = if child_elem.property_name.is_some() {
            child_elem.property_name
        } else {
            child_elem.name
        };
        let Some(child_key_node) = self.arena.get(child_key_idx) else {
            return false;
        };
        if child_key_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }

        let value_name = self.get_temp_var_name();
        self.write(&value_name);
        self.write(" = ");
        if allow_expression_emit {
            self.emit(initializer);
        } else {
            self.emit_expression(initializer);
        }
        self.write(".");
        self.write_identifier_text(key_idx);
        self.write(".");
        self.write_identifier_text(child_key_idx);

        if child_elem.initializer.is_none() {
            self.write(", ");
            self.write_binding_identifier_text(child_elem.name);
            self.write(" = ");
            self.write(&value_name);
        } else {
            self.write(", ");
            self.write_binding_identifier_text(child_elem.name);
            self.write(" = ");
            self.write(&value_name);
            self.write(" === void 0 ? ");
            self.emit_expression(child_elem.initializer);
            self.write(" : ");
            self.write(&value_name);
        }
        true
    }

    // Binding element patterns + param bindings → es5/bindings_patterns.rs
    // For-of array + assignment destructuring → es5/bindings_assignment.rs
}
