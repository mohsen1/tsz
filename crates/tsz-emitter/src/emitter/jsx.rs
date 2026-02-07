use super::Printer;
use tsz_parser::parser::node::Node;

impl<'a> Printer<'a> {
    // =========================================================================
    // JSX
    // =========================================================================

    pub(super) fn emit_jsx_element(&mut self, node: &Node) {
        let Some(jsx) = self.arena.get_jsx_element(node) else {
            return;
        };

        self.emit(jsx.opening_element);
        for &child in &jsx.children.nodes {
            self.emit(child);
        }
        self.emit(jsx.closing_element);
    }

    pub(super) fn emit_jsx_self_closing_element(&mut self, node: &Node) {
        let Some(jsx) = self.arena.get_jsx_opening(node) else {
            return;
        };

        self.write("<");
        self.emit(jsx.tag_name);
        self.emit(jsx.attributes);
        self.write(" />");
    }

    pub(super) fn emit_jsx_opening_element(&mut self, node: &Node) {
        let Some(jsx) = self.arena.get_jsx_opening(node) else {
            return;
        };

        self.write("<");
        self.emit(jsx.tag_name);
        self.emit(jsx.attributes);
        self.write(">");
    }

    pub(super) fn emit_jsx_closing_element(&mut self, node: &Node) {
        let Some(jsx) = self.arena.get_jsx_closing(node) else {
            return;
        };

        self.write("</");
        self.emit(jsx.tag_name);
        self.write(">");
    }

    pub(super) fn emit_jsx_fragment(&mut self, node: &Node) {
        let Some(jsx) = self.arena.get_jsx_fragment(node) else {
            return;
        };

        self.write("<>");
        for &child in &jsx.children.nodes {
            self.emit(child);
        }
        self.write("</>");
    }

    pub(super) fn emit_jsx_attributes(&mut self, node: &Node) {
        let Some(attrs) = self.arena.get_jsx_attributes(node) else {
            return;
        };

        for &attr in &attrs.properties.nodes {
            self.write_space();
            self.emit(attr);
        }
    }

    pub(super) fn emit_jsx_attribute(&mut self, node: &Node) {
        let Some(attr) = self.arena.get_jsx_attribute(node) else {
            return;
        };

        self.emit(attr.name);
        if !attr.initializer.is_none() {
            self.write("=");
            self.emit(attr.initializer);
        }
    }

    pub(super) fn emit_jsx_spread_attribute(&mut self, node: &Node) {
        let Some(spread) = self.arena.get_jsx_spread_attribute(node) else {
            return;
        };

        self.write("{...");
        self.emit(spread.expression);
        self.write("}");
    }

    pub(super) fn emit_jsx_expression(&mut self, node: &Node) {
        let Some(expr) = self.arena.get_jsx_expression(node) else {
            return;
        };

        self.write("{");
        if expr.dot_dot_dot_token {
            self.write("...");
        }
        self.emit(expr.expression);
        self.write("}");
    }

    pub(super) fn emit_jsx_text(&mut self, node: &Node) {
        let Some(text) = self.arena.get_jsx_text(node) else {
            return;
        };

        self.write(&text.text);
    }

    pub(super) fn emit_jsx_namespaced_name(&mut self, node: &Node) {
        let Some(ns) = self.arena.get_jsx_namespaced_name(node) else {
            return;
        };

        self.emit(ns.namespace);
        self.write(":");
        self.emit(ns.name);
    }
}
