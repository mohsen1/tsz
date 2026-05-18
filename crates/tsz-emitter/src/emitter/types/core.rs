use crate::emitter::Printer;
use tsz_parser::parser::node::Node;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    // =========================================================================
    // Types
    // =========================================================================

    pub(in crate::emitter) fn emit_type_reference(&mut self, node: &Node) {
        let Some(type_ref) = self.arena.get_type_ref(node) else {
            return;
        };

        self.emit(type_ref.type_name);

        if let Some(ref type_args) = type_ref.type_arguments
            && !type_args.nodes.is_empty()
        {
            self.write("<");
            self.emit_comma_separated(&type_args.nodes);
            self.write(">");
        }
    }

    pub(in crate::emitter) fn emit_union_type(&mut self, node: &Node) {
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

    pub(in crate::emitter) fn emit_intersection_type(&mut self, node: &Node) {
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

    pub(in crate::emitter) fn emit_array_type(&mut self, node: &Node) {
        let Some(array) = self.arena.get_array_type(node) else {
            return;
        };

        self.emit(array.element_type);
        self.write("[]");
    }

    pub(in crate::emitter) fn emit_tuple_type(&mut self, node: &Node) {
        let Some(tuple) = self.arena.get_tuple_type(node) else {
            self.write("[]");
            return;
        };

        self.write("[");
        self.emit_comma_separated(&tuple.elements.nodes);
        self.write("]");
    }

    pub(in crate::emitter) fn emit_function_type(&mut self, node: &Node) {
        let Some(func_type) = self.arena.get_function_type(node) else {
            return;
        };

        // Type parameters
        if let Some(ref type_params) = func_type.type_parameters
            && !type_params.nodes.is_empty()
        {
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

    pub(in crate::emitter) fn emit_constructor_type(&mut self, node: &Node) {
        let Some(func_type) = self.arena.get_function_type(node) else {
            return;
        };

        // Abstract modifier
        if func_type.is_abstract {
            self.write("abstract ");
        }

        self.write("new ");

        // Type parameters
        if let Some(ref type_params) = func_type.type_parameters
            && !type_params.nodes.is_empty()
        {
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

    pub(in crate::emitter) fn emit_type_literal(&mut self, node: &Node) {
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

    pub(in crate::emitter) fn emit_parenthesized_type(&mut self, node: &Node) {
        let Some(paren_type) = self.arena.get_wrapped_type(node) else {
            return;
        };

        self.write("(");
        self.emit(paren_type.type_node);
        self.write(")");
    }

    pub(in crate::emitter) fn emit_type_parameter(&mut self, node: &Node) {
        let Some(param) = self.arena.get_type_parameter(node) else {
            return;
        };

        // Emit variance/const modifiers (in, out, const)
        if let Some(ref mods) = param.modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    match mod_node.kind {
                        k if k == SyntaxKind::InKeyword as u16 => self.write("in "),
                        k if k == SyntaxKind::OutKeyword as u16 => self.write("out "),
                        k if k == SyntaxKind::ConstKeyword as u16 => self.write("const "),
                        _ => {}
                    }
                }
            }
        }

        self.emit(param.name);

        if param.constraint.is_some() {
            self.write(" extends ");
            self.emit(param.constraint);
        }

        if param.default.is_some() {
            self.write(" = ");
            self.emit(param.default);
        }
    }

    // =========================================================================
    // Interface/Type Members (Signatures)
    // =========================================================================

    pub(in crate::emitter) fn emit_property_signature(&mut self, node: &Node) {
        let Some(sig) = self.arena.get_signature(node) else {
            return;
        };

        // Emit modifiers (readonly)
        self.emit_class_member_modifiers(&sig.modifiers);

        if sig.name.is_some() {
            self.emit(sig.name);
        }

        if sig.question_token {
            self.write("?");
        }

        if sig.type_annotation.is_some() {
            self.write(": ");
            self.emit(sig.type_annotation);
        }
    }

    pub(in crate::emitter) fn emit_method_signature(&mut self, node: &Node) {
        let Some(sig) = self.arena.get_signature(node) else {
            return;
        };

        if sig.name.is_some() {
            self.emit(sig.name);
        }

        if sig.question_token {
            self.write("?");
        }

        // Type parameters (e.g., method<T>(x: T): T)
        if let Some(ref type_params) = sig.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.write("<");
            self.emit_comma_separated(&type_params.nodes);
            self.write(">");
        }

        self.write("(");
        if let Some(ref params) = sig.parameters {
            self.emit_comma_separated(&params.nodes);
        }
        self.write(")");

        if sig.type_annotation.is_some() {
            self.write(": ");
            self.emit(sig.type_annotation);
        }
    }

    pub(in crate::emitter) fn emit_call_signature(&mut self, node: &Node) {
        let Some(sig) = self.arena.get_signature(node) else {
            return;
        };

        if let Some(ref type_params) = sig.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.write("<");
            self.emit_comma_separated(&type_params.nodes);
            self.write(">");
        }

        self.write("(");
        if let Some(ref params) = sig.parameters {
            self.emit_comma_separated(&params.nodes);
        }
        self.write(")");

        if sig.type_annotation.is_some() {
            self.write(": ");
            self.emit(sig.type_annotation);
        }
    }

    pub(in crate::emitter) fn emit_construct_signature(&mut self, node: &Node) {
        let Some(sig) = self.arena.get_signature(node) else {
            return;
        };

        self.write("new ");

        if let Some(ref type_params) = sig.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.write("<");
            self.emit_comma_separated(&type_params.nodes);
            self.write(">");
        }

        self.write("(");
        if let Some(ref params) = sig.parameters {
            self.emit_comma_separated(&params.nodes);
        }
        self.write(")");

        if sig.type_annotation.is_some() {
            self.write(": ");
            self.emit(sig.type_annotation);
        }
    }

    pub(in crate::emitter) fn emit_index_signature(&mut self, node: &Node) {
        let Some(sig) = self.arena.get_index_signature(node) else {
            return;
        };

        // Emit modifiers (readonly)
        self.emit_class_member_modifiers(&sig.modifiers);

        self.write("[");
        self.emit_comma_separated(&sig.parameters.nodes);
        self.write("]");

        if sig.type_annotation.is_some() {
            self.write(": ");
            self.emit(sig.type_annotation);
        }
    }

    // =========================================================================
    // Additional type nodes for complete DTS passthrough support
    // =========================================================================

    pub(in crate::emitter) fn emit_conditional_type(&mut self, node: &Node) {
        let Some(cond) = self.arena.get_conditional_type(node) else {
            return;
        };
        self.emit(cond.check_type);
        self.write(" extends ");
        self.emit(cond.extends_type);
        self.write(" ? ");
        self.emit(cond.true_type);
        self.write(" : ");
        self.emit(cond.false_type);
    }

    pub(in crate::emitter) fn emit_indexed_access_type(&mut self, node: &Node) {
        let Some(idx) = self.arena.get_indexed_access_type(node) else {
            return;
        };
        self.emit(idx.object_type);
        self.write("[");
        self.emit(idx.index_type);
        self.write("]");
    }

    pub(in crate::emitter) fn emit_infer_type(&mut self, node: &Node) {
        let Some(infer) = self.arena.get_infer_type(node) else {
            return;
        };
        self.write("infer ");
        self.emit(infer.type_parameter);
    }

    pub(in crate::emitter) fn emit_literal_type(&mut self, node: &Node) {
        let Some(lit_type) = self.arena.get_literal_type(node) else {
            return;
        };
        self.emit(lit_type.literal);
    }

    pub(in crate::emitter) fn emit_mapped_type(&mut self, node: &Node) {
        let Some(mapped) = self.arena.get_mapped_type(node) else {
            return;
        };
        self.write("{");
        self.write_line();
        self.increase_indent();

        // Readonly modifier
        if let Some(rt_node) = self.arena.get(mapped.readonly_token) {
            match rt_node.kind {
                k if k == SyntaxKind::PlusToken as u16 => self.write("+readonly "),
                k if k == SyntaxKind::MinusToken as u16 => self.write("-readonly "),
                _ => self.write("readonly "),
            }
        }

        self.write("[");
        if let Some(tp_node) = self.arena.get(mapped.type_parameter)
            && let Some(tp) = self.arena.get_type_parameter(tp_node)
        {
            self.emit(tp.name);
            self.write(" in ");
            if tp.constraint.is_some() {
                self.emit(tp.constraint);
            }
        }
        if mapped.name_type.is_some() {
            self.write(" as ");
            self.emit(mapped.name_type);
        }
        self.write("]");

        // Question modifier
        if let Some(qt_node) = self.arena.get(mapped.question_token) {
            match qt_node.kind {
                k if k == SyntaxKind::PlusToken as u16 => self.write("+?"),
                k if k == SyntaxKind::MinusToken as u16 => self.write("-?"),
                _ => self.write("?"),
            }
        }

        self.write(": ");
        self.emit(mapped.type_node);
        self.write(";");
        self.write_line();
        self.decrease_indent();
        self.write("}");
    }

    pub(in crate::emitter) fn emit_named_tuple_member(&mut self, node: &Node) {
        let Some(member) = self.arena.get_named_tuple_member(node) else {
            return;
        };
        if member.dot_dot_dot_token {
            self.write("...");
        }
        self.emit(member.name);
        if member.question_token {
            self.write("?");
        }
        self.write(": ");
        self.emit(member.type_node);
    }

    pub(in crate::emitter) fn emit_optional_type(&mut self, node: &Node) {
        let Some(wrapped) = self.arena.get_wrapped_type(node) else {
            return;
        };
        self.emit(wrapped.type_node);
        self.write("?");
    }

    pub(in crate::emitter) fn emit_rest_type(&mut self, node: &Node) {
        let Some(wrapped) = self.arena.get_wrapped_type(node) else {
            return;
        };
        self.write("...");
        self.emit(wrapped.type_node);
    }

    pub(in crate::emitter) fn emit_template_literal_type(&mut self, node: &Node) {
        let Some(tlt) = self.arena.get_template_literal_type(node) else {
            return;
        };
        if let Some(head_node) = self.arena.get(tlt.head)
            && let Some(lit) = self.arena.get_literal(head_node)
        {
            if head_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16 {
                self.write("`");
                self.write(&lit.text);
                self.write("`");
            } else {
                self.write("`");
                self.write(&lit.text);
                self.write("${");
            }
        }
        for (i, &span_idx) in tlt.template_spans.nodes.iter().enumerate() {
            if let Some(span_node) = self.arena.get(span_idx)
                && let Some(span) = self.arena.get_template_span(span_node)
            {
                self.emit(span.expression);
                if let Some(lit_node) = self.arena.get(span.literal)
                    && let Some(lit) = self.arena.get_literal(lit_node)
                {
                    let is_last = i == tlt.template_spans.nodes.len() - 1;
                    self.write("}");
                    self.write(&lit.text);
                    if is_last {
                        self.write("`");
                    } else {
                        self.write("${");
                    }
                }
            }
        }
    }

    pub(in crate::emitter) fn emit_type_operator(&mut self, node: &Node) {
        let Some(type_op) = self.arena.get_type_operator(node) else {
            return;
        };
        if type_op.operator == SyntaxKind::KeyOfKeyword as u16 {
            self.write("keyof ");
        } else if type_op.operator == SyntaxKind::ReadonlyKeyword as u16 {
            self.write("readonly ");
        } else if type_op.operator == SyntaxKind::UniqueKeyword as u16 {
            self.write("unique ");
        }
        self.emit(type_op.type_node);
    }

    pub(in crate::emitter) fn emit_type_predicate(&mut self, node: &Node) {
        let Some(pred) = self.arena.get_type_predicate(node) else {
            return;
        };
        if pred.asserts_modifier {
            self.write("asserts ");
        }
        self.emit(pred.parameter_name);
        if pred.type_node.is_some() && self.arena.get(pred.type_node).is_some_and(|n| n.kind != 1) {
            self.write(" is ");
            self.emit(pred.type_node);
        }
    }

    pub(in crate::emitter) fn emit_type_query(&mut self, node: &Node) {
        self.write("typeof ");
        if let Some(tq) = self.arena.get_type_query(node) {
            self.emit(tq.expr_name);
            if let Some(ref type_args) = tq.type_arguments
                && !type_args.nodes.is_empty()
            {
                self.write("<");
                self.emit_comma_separated(&type_args.nodes);
                self.write(">");
            }
        }
    }
}
