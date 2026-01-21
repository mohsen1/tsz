use super::Printer;
use crate::parser::node::Node;

impl<'a> Printer<'a> {
    // =========================================================================
    // Types
    // =========================================================================

    pub(super) fn emit_type_reference(&mut self, node: &Node) {
        let Some(type_ref) = self.arena.get_type_ref(node) else {
            return;
        };

        self.emit(type_ref.type_name);

        if let Some(ref type_args) = type_ref.type_arguments
            && !type_args.nodes.is_empty() {
                self.write("<");
                self.emit_comma_separated(&type_args.nodes);
                self.write(">");
            }
    }

    pub(super) fn emit_union_type(&mut self, node: &Node) {
        let Some(union) = self.arena.get_composite_type(node) else {
            return;
        };

        let mut first = true;
        for &type_idx in &union.types.nodes {
            if !first {
                self.write(" | ");
            }
            first = false;
            self.emit(type_idx);
        }
    }

    pub(super) fn emit_intersection_type(&mut self, node: &Node) {
        let Some(intersection) = self.arena.get_composite_type(node) else {
            return;
        };

        let mut first = true;
        for &type_idx in &intersection.types.nodes {
            if !first {
                self.write(" & ");
            }
            first = false;
            self.emit(type_idx);
        }
    }

    pub(super) fn emit_array_type(&mut self, node: &Node) {
        let Some(array) = self.arena.get_array_type(node) else {
            return;
        };

        self.emit(array.element_type);
        self.write("[]");
    }

    pub(super) fn emit_tuple_type(&mut self, node: &Node) {
        let Some(tuple) = self.arena.get_tuple_type(node) else {
            self.write("[]");
            return;
        };

        self.write("[");
        self.emit_comma_separated(&tuple.elements.nodes);
        self.write("]");
    }

    pub(super) fn emit_function_type(&mut self, node: &Node) {
        let Some(func_type) = self.arena.get_function_type(node) else {
            return;
        };

        // Type parameters
        if let Some(ref type_params) = func_type.type_parameters
            && !type_params.nodes.is_empty() {
                self.write("<");
                self.emit_comma_separated(&type_params.nodes);
                self.write(">");
            }

        // Parameters
        self.write("(");
        self.emit_comma_separated(&func_type.parameters.nodes);
        self.write(") => ");

        // Return type
        self.emit(func_type.type_annotation);
    }

    pub(super) fn emit_type_literal(&mut self, node: &Node) {
        let Some(type_lit) = self.arena.get_type_literal(node) else {
            self.write("{}");
            return;
        };

        if type_lit.members.nodes.is_empty() {
            self.write("{}");
            return;
        }

        self.write("{");
        self.write_line();
        self.increase_indent();

        for &member_idx in &type_lit.members.nodes {
            self.emit(member_idx);
            self.write_semicolon();
            self.write_line();
        }

        self.decrease_indent();
        self.write("}");
    }

    pub(super) fn emit_parenthesized_type(&mut self, node: &Node) {
        let Some(paren_type) = self.arena.get_wrapped_type(node) else {
            return;
        };

        self.write("(");
        self.emit(paren_type.type_node);
        self.write(")");
    }

    pub(super) fn emit_type_parameter(&mut self, node: &Node) {
        let Some(param) = self.arena.get_type_parameter(node) else {
            return;
        };

        self.emit(param.name);

        if !param.constraint.is_none() {
            self.write(" extends ");
            self.emit(param.constraint);
        }

        if !param.default.is_none() {
            self.write(" = ");
            self.emit(param.default);
        }
    }

    // =========================================================================
    // Interface/Type Members (Signatures)
    // =========================================================================

    pub(super) fn emit_property_signature(&mut self, node: &Node) {
        let Some(sig) = self.arena.get_signature(node) else {
            return;
        };

        // Emit modifiers (readonly)
        self.emit_class_member_modifiers(&sig.modifiers);

        if !sig.name.is_none() {
            self.emit(sig.name);
        }

        if sig.question_token {
            self.write("?");
        }

        if !sig.type_annotation.is_none() {
            self.write(": ");
            self.emit(sig.type_annotation);
        }
    }

    pub(super) fn emit_method_signature(&mut self, node: &Node) {
        let Some(sig) = self.arena.get_signature(node) else {
            return;
        };

        if !sig.name.is_none() {
            self.emit(sig.name);
        }

        if sig.question_token {
            self.write("?");
        }

        self.write("(");
        if let Some(ref params) = sig.parameters {
            self.emit_comma_separated(&params.nodes);
        }
        self.write(")");

        if !sig.type_annotation.is_none() {
            self.write(": ");
            self.emit(sig.type_annotation);
        }
    }

    pub(super) fn emit_call_signature(&mut self, node: &Node) {
        let Some(sig) = self.arena.get_signature(node) else {
            return;
        };

        if let Some(ref type_params) = sig.type_parameters
            && !type_params.nodes.is_empty() {
                self.write("<");
                self.emit_comma_separated(&type_params.nodes);
                self.write(">");
            }

        self.write("(");
        if let Some(ref params) = sig.parameters {
            self.emit_comma_separated(&params.nodes);
        }
        self.write(")");

        if !sig.type_annotation.is_none() {
            self.write(": ");
            self.emit(sig.type_annotation);
        }
    }

    pub(super) fn emit_construct_signature(&mut self, node: &Node) {
        let Some(sig) = self.arena.get_signature(node) else {
            return;
        };

        self.write("new ");

        if let Some(ref type_params) = sig.type_parameters
            && !type_params.nodes.is_empty() {
                self.write("<");
                self.emit_comma_separated(&type_params.nodes);
                self.write(">");
            }

        self.write("(");
        if let Some(ref params) = sig.parameters {
            self.emit_comma_separated(&params.nodes);
        }
        self.write(")");

        if !sig.type_annotation.is_none() {
            self.write(": ");
            self.emit(sig.type_annotation);
        }
    }

    pub(super) fn emit_index_signature(&mut self, node: &Node) {
        let Some(sig) = self.arena.get_index_signature(node) else {
            return;
        };

        // Emit modifiers (readonly)
        self.emit_class_member_modifiers(&sig.modifiers);

        self.write("[");
        self.emit_comma_separated(&sig.parameters.nodes);
        self.write("]");

        if !sig.type_annotation.is_none() {
            self.write(": ");
            self.emit(sig.type_annotation);
        }
    }
}
