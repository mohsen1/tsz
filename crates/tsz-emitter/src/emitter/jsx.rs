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
        // tsc always emits writeSpace() after tagName. Our emit_jsx_attributes
        // already prepends a space before each attribute, so the space is only
        // missing when there are no attributes at all.
        let has_attributes = self
            .arena
            .get(jsx.attributes)
            .and_then(|n| self.arena.get_jsx_attributes(n))
            .is_some_and(|a| !a.properties.nodes.is_empty());
        if !has_attributes {
            self.write(" ");
        }
        self.write("/>");
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
        if attr.initializer.is_some() {
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

#[cfg(test)]
mod tests {
    use crate::printer::{PrintOptions, Printer};
    use tsz_parser::ParserState;

    fn emit_jsx(source: &str) -> String {
        let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        printer.finish().code
    }

    #[test]
    fn self_closing_no_attributes_has_space_before_slash() {
        let output = emit_jsx("const x = <Tag />;");
        assert!(
            output.contains("<Tag />"),
            "Self-closing element without attributes should have space before />.\nOutput: {output}"
        );
    }

    #[test]
    fn self_closing_with_attributes_has_no_space_before_slash() {
        let output = emit_jsx("const x = <Tag foo=\"bar\"/>;");
        assert!(
            output.contains("<Tag foo=\"bar\"/>"),
            "Self-closing element with attributes should NOT have extra space before />.\nOutput: {output}"
        );
    }

    #[test]
    fn self_closing_with_expression_attribute_no_extra_space() {
        let output = emit_jsx("const x = <Tag value={42}/>;");
        assert!(
            output.contains("<Tag value={42}/>"),
            "Self-closing element with expression attribute should NOT have extra space before />.\nOutput: {output}"
        );
    }

    #[test]
    fn jsx_text_multiline_content_preserves_whitespace() {
        // tsc preserves JSX text content including leading/trailing whitespace and newlines.
        // The scanner's re_scan_jsx_token must reset to full_start_pos (before trivia)
        // so the text node captures the complete whitespace content.
        let source = "let k1 = <Comp a={10} b=\"hi\">\n        hi hi hi!\n    </Comp>;";
        let output = emit_jsx(source);
        assert!(
            output.contains("\n        hi hi hi!\n    "),
            "JSX text should preserve leading/trailing whitespace and newlines.\nOutput: {output}"
        );
    }

    #[test]
    fn jsx_text_single_line_content() {
        let output = emit_jsx("let x = <div>hello world</div>;");
        assert!(
            output.contains(">hello world</"),
            "JSX text on single line should be preserved.\nOutput: {output}"
        );
    }

    #[test]
    fn jsx_text_with_nested_elements() {
        let source = "let x = <Comp>\n        <div>inner</div>\n    </Comp>;";
        let output = emit_jsx(source);
        assert!(
            output.contains("\n        <div>inner</div>\n    "),
            "JSX text whitespace around nested elements should be preserved.\nOutput: {output}"
        );
    }

    #[test]
    fn jsx_text_whitespace_only_between_elements() {
        // Whitespace-only text nodes between JSX elements should be preserved
        let source = "let x = <div>\n    <span>a</span>\n    <span>b</span>\n</div>;";
        let output = emit_jsx(source);
        assert!(
            output.contains("<span>a</span>\n    <span>b</span>"),
            "Whitespace between JSX children should be preserved.\nOutput: {output}"
        );
    }
}
