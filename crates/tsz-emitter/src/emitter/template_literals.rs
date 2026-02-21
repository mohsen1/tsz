use super::Printer;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;

impl<'a> Printer<'a> {
    // =========================================================================
    // Template Literals
    // =========================================================================

    pub(super) fn emit_tagged_template_expression(&mut self, node: &Node, _idx: NodeIndex) {
        let Some(tagged) = self.arena.get_tagged_template(node) else {
            return;
        };

        self.emit_expression(tagged.tag);
        self.write_space();
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
        let text = self
            .get_raw_template_part_text(node)
            .or_else(|| self.arena.get_literal(node).map(|lit| lit.text.clone()))
            .unwrap_or_default();
        self.write("`");
        self.write(&text);
        self.write("`");
    }

    pub(super) fn emit_template_span(&mut self, node: &Node) {
        let Some(span) = self.arena.get_template_span(node) else {
            return;
        };

        // Emit ${expression}
        self.write("${");
        self.emit(span.expression);
        if self.template_span_has_closing_brace(span) {
            self.write("}");
        }
        // Emit the literal part (middle or tail)
        self.emit(span.literal);
    }

    pub(super) fn emit_template_head(&mut self, node: &Node) {
        let text = self
            .get_raw_template_part_text(node)
            .or_else(|| self.arena.get_literal(node).map(|lit| lit.text.clone()))
            .unwrap_or_default();
        // Template head starts with ` and ends with ${
        self.write("`");
        self.write(&text);
    }

    pub(super) fn emit_template_middle(&mut self, node: &Node) {
        let text = self
            .get_raw_template_part_text(node)
            .or_else(|| self.arena.get_literal(node).map(|lit| lit.text.clone()))
            .unwrap_or_default();
        // Template middle is between } and ${
        self.write(&text);
    }

    pub(super) fn emit_template_tail(&mut self, node: &Node) {
        let text = self
            .get_raw_template_part_text(node)
            .or_else(|| self.arena.get_literal(node).map(|lit| lit.text.clone()))
            .unwrap_or_default();
        // Template tail ends with `
        self.write(&text);
        if self.template_tail_has_backtick(node) {
            self.write("`");
        }
    }

    pub(super) fn get_raw_template_part_text(&self, node: &Node) -> Option<String> {
        let text = self.source_text?;
        let cooked_fallback = self
            .arena
            .get_literal(node)
            .map(|lit| lit.text.clone())
            .unwrap_or_default();

        let (skip_leading, allow_dollar_brace, allow_backtick) = match node.kind {
            k if k == tsz_scanner::SyntaxKind::NoSubstitutionTemplateLiteral as u16 => {
                (1_usize, false, true)
            }
            k if k == tsz_scanner::SyntaxKind::TemplateHead as u16 => (1_usize, true, true),
            k if k == tsz_scanner::SyntaxKind::TemplateMiddle as u16 => (1_usize, true, true),
            k if k == tsz_scanner::SyntaxKind::TemplateTail as u16 => (1_usize, false, true),
            _ => return Some(cooked_fallback),
        };

        let start = node.pos as usize;
        if start >= text.len() || start + skip_leading >= text.len() {
            return Some(cooked_fallback);
        }

        let bytes = text.as_bytes();
        let mut i = start + skip_leading;
        while i < bytes.len() {
            let ch = bytes[i];
            if ch == b'\\' {
                i += 1;
                if i < bytes.len() {
                    i += 1;
                }
                continue;
            }

            if allow_backtick && ch == b'`' {
                return Some(text[start + skip_leading..i].to_string());
            }

            if allow_dollar_brace && ch == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                return Some(text[start + skip_leading..i].to_string());
            }

            i += 1;
        }

        Some(cooked_fallback)
    }

    fn template_span_has_closing_brace(
        &self,
        span: &tsz_parser::parser::node::TemplateSpanData,
    ) -> bool {
        let Some(text) = self.source_text else {
            return true;
        };
        let Some(expr_node) = self.arena.get(span.expression) else {
            return true;
        };
        let Some(lit_node) = self.arena.get(span.literal) else {
            return false;
        };

        let start = std::cmp::min(expr_node.end as usize, text.len());
        let end = std::cmp::min(lit_node.pos as usize, text.len());
        if start < end {
            return text[start..end].contains('}');
        }

        if start < text.len() && text.as_bytes()[start] == b'}' {
            return true;
        }
        if end < text.len() && text.as_bytes()[end] == b'}' {
            return true;
        }

        let boundary = std::cmp::min(end, text.len());
        if boundary > 0 {
            let bytes = text.as_bytes();
            let mut i = boundary;
            while i > 0 && bytes[i - 1].is_ascii_whitespace() {
                i -= 1;
            }
            if i > 0 && bytes[i - 1] == b'}' {
                return true;
            }
        }

        false
    }

    fn template_tail_has_backtick(&self, node: &Node) -> bool {
        let Some(text) = self.source_text else {
            return true;
        };
        let start = std::cmp::min(node.pos as usize, text.len());
        let end = std::cmp::min(node.end as usize, text.len());

        if start < end && text[start..end].contains('`') {
            return true;
        }

        end < text.len() && text.as_bytes()[end] == b'`'
    }
}
