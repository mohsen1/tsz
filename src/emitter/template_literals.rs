use super::Printer;
use crate::parser::NodeIndex;
use crate::parser::node::Node;

impl<'a> Printer<'a> {
    // =========================================================================
    // Template Literals
    // =========================================================================

    pub(super) fn emit_tagged_template_expression(&mut self, node: &Node, _idx: NodeIndex) {
        let Some(tagged) = self.arena.get_tagged_template(node) else {
            return;
        };

        self.emit_expression(tagged.tag);
        self.emit(tagged.template);
    }

    pub(super) fn emit_template_expression(&mut self, node: &Node) {
        let Some(tpl) = self.arena.get_template_expr(node) else {
            self.write("``");
            return;
        };

        // Emit the template head (opening backtick and initial text)
        self.emit(tpl.head);

        // Emit each template span (expression + middle/tail)
        for &span_idx in &tpl.template_spans.nodes {
            self.emit(span_idx);
        }
    }

    pub(super) fn emit_no_substitution_template(&mut self, node: &Node) {
        if let Some(lit) = self.arena.get_literal(node) {
            self.write("`");
            self.write(&lit.text);
            self.write("`");
        }
    }

    pub(super) fn emit_template_span(&mut self, node: &Node) {
        let Some(span) = self.arena.get_template_span(node) else {
            return;
        };

        // Emit ${expression}
        self.write("${");
        self.emit(span.expression);
        self.write("}");
        // Emit the literal part (middle or tail)
        self.emit(span.literal);
    }

    pub(super) fn emit_template_head(&mut self, node: &Node) {
        if let Some(lit) = self.arena.get_literal(node) {
            // Template head starts with ` and ends with ${
            self.write("`");
            self.write(&lit.text);
        }
    }

    pub(super) fn emit_template_middle(&mut self, node: &Node) {
        if let Some(lit) = self.arena.get_literal(node) {
            // Template middle is between } and ${
            self.write(&lit.text);
        }
    }

    pub(super) fn emit_template_tail(&mut self, node: &Node) {
        if let Some(lit) = self.arena.get_literal(node) {
            // Template tail ends with `
            self.write(&lit.text);
            self.write("`");
        }
    }
}
