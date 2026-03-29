use crate::emitter::Printer;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    // =========================================================================
    // Template Literals
    // =========================================================================

    pub(in crate::emitter) fn emit_tagged_template_expression(
        &mut self,
        node: &Node,
        _idx: NodeIndex,
    ) {
        let Some(tagged) = self.arena.get_tagged_template(node) else {
            return;
        };

        self.emit_expression(tagged.tag);

        // When the tag is `super` with type arguments (which are stripped),
        // tsc emits `super. ` to preserve the intent of a property access.
        if tagged.type_arguments.is_some() {
            if let Some(tag_node) = self.arena.get(tagged.tag) {
                if tag_node.kind == SyntaxKind::SuperKeyword as u16 {
                    self.write(".");
                }
            }
        }

        self.write_space();
        self.emit(tagged.template);
    }

    pub(in crate::emitter) fn emit_template_expression(&mut self, node: &Node) {
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

    pub(in crate::emitter) fn emit_no_substitution_template(&mut self, node: &Node) {
        let text = self
            .get_raw_template_part_text(node)
            .or_else(|| self.arena.get_literal(node).map(|lit| lit.text.clone()))
            .unwrap_or_default();
        self.write("`");
        self.write(&text);
        if self.no_substitution_template_has_closing_backtick(node) {
            self.write("`");
        }
    }

    pub(in crate::emitter) fn emit_template_span(&mut self, node: &Node) {
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

    pub(in crate::emitter) fn emit_template_head(&mut self, node: &Node) {
        let text = self
            .get_raw_template_part_text(node)
            .or_else(|| self.arena.get_literal(node).map(|lit| lit.text.clone()))
            .unwrap_or_default();
        // Template head starts with ` and ends with ${
        self.write("`");
        self.write(&text);
    }

    pub(in crate::emitter) fn emit_template_middle(&mut self, node: &Node) {
        let text = self
            .get_raw_template_part_text(node)
            .or_else(|| self.arena.get_literal(node).map(|lit| lit.text.clone()))
            .unwrap_or_default();
        // Template middle is between } and ${
        self.write(&text);
    }

    pub(in crate::emitter) fn emit_template_tail(&mut self, node: &Node) {
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

    pub(in crate::emitter) fn get_raw_template_part_text(&self, node: &Node) -> Option<String> {
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

        // No closing delimiter found — unterminated template literal.
        // Use the raw source text from after the opening backtick to the end
        // of the node range, preserving escape sequences verbatim.
        let end = std::cmp::min(node.end as usize, text.len());
        let content_start = start + skip_leading;
        if content_start <= end {
            Some(text[content_start..end].to_string())
        } else {
            Some(cooked_fallback)
        }
    }

    /// Check whether the source text has a closing `}` for this template span.
    ///
    /// The literal node (TemplateMiddle/TemplateTail) starts at the `}` character
    /// in the source. We check `lit_node.pos` directly, then scan the range
    /// between `expr_node.end` and `lit_node.pos` (inclusive) as a fallback.
    fn template_span_has_closing_brace(
        &self,
        span: &tsz_parser::parser::node::TemplateSpanData,
    ) -> bool {
        let Some(text) = self.source_text else {
            return true;
        };
        let Some(lit_node) = self.arena.get(span.literal) else {
            return false;
        };

        // The literal node's pos should be at the `}` character.
        let pos = lit_node.pos as usize;
        if pos < text.len() && text.as_bytes()[pos] == b'}' {
            return true;
        }

        // Fallback: scan backwards from the literal position past whitespace.
        if pos > 0 {
            let bytes = text.as_bytes();
            let mut i = pos;
            while i > 0 && bytes[i - 1].is_ascii_whitespace() {
                i -= 1;
            }
            if i > 0 && bytes[i - 1] == b'}' {
                return true;
            }
        }

        false
    }

    /// Check whether the source text has a closing backtick for a
    /// `NoSubstitutionTemplateLiteral`. When the template is unterminated
    /// (error recovery), the source text does not contain a closing backtick
    /// and the emitter should omit it to match tsc behavior.
    fn no_substitution_template_has_closing_backtick(&self, node: &Node) -> bool {
        let Some(text) = self.source_text else {
            return true;
        };
        let end = node.end as usize;
        let start = node.pos as usize;
        let bytes = text.as_bytes();

        // The closing backtick should be at end - 1.
        if end == 0 || end > bytes.len() || bytes[end - 1] != b'`' {
            return false;
        }

        // Make sure it's not the OPENING backtick (single-char unterminated `).
        if end - 1 == start {
            return false;
        }

        // Count consecutive backslashes immediately before the backtick.
        // An odd number means the backtick is escaped (\`), not a closing delimiter.
        let mut backslash_count = 0usize;
        let mut p = end - 2;
        while p > start && bytes[p] == b'\\' {
            backslash_count += 1;
            p -= 1;
        }

        backslash_count.is_multiple_of(2)
    }

    /// Check whether the source text has a closing backtick for a `TemplateTail`.
    ///
    /// The `TemplateTail` node spans from `}` through the closing `` ` ``. The
    /// backtick should be the last character at `node.end - 1`.
    fn template_tail_has_backtick(&self, node: &Node) -> bool {
        let Some(text) = self.source_text else {
            return true;
        };
        let end = node.end as usize;

        // The backtick should be at end - 1 (the last character of the tail token).
        if end > 0 && end <= text.len() && text.as_bytes()[end - 1] == b'`' {
            return true;
        }

        // Fallback: check within the node's range.
        let start = std::cmp::min(node.pos as usize, text.len());
        let end_clamped = std::cmp::min(end, text.len());
        if start < end_clamped && text[start..end_clamped].contains('`') {
            return true;
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use crate::output::printer::{PrintOptions, Printer};
    use tsz_parser::ParserState;

    fn emit(source: &str) -> String {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        printer.finish().code
    }

    /// Unterminated template literal: just an opening backtick with no closing.
    /// tsc preserves the unterminated form verbatim — no closing backtick added.
    /// The emitter adds `;` as an expression statement terminator.
    #[test]
    fn unterminated_template_no_content() {
        let output = emit("`");
        assert_eq!(
            output.trim(),
            "`;",
            "should emit opening backtick without closing, plus statement semicolon"
        );
    }

    /// Unterminated template with an escaped backtick (backslash + backtick).
    /// The backslash-backtick is content, not a closing delimiter.
    #[test]
    fn unterminated_template_escaped_backtick() {
        let output = emit("`\\`");
        assert_eq!(
            output.trim(),
            "`\\`;",
            "escaped backtick should not close the template"
        );
    }

    /// Unterminated template with double backslash (`\\`).
    /// Two backslashes are self-escaping; no closing backtick present.
    #[test]
    fn unterminated_template_double_backslash() {
        let output = emit("`\\\\");
        assert_eq!(
            output.trim(),
            "`\\\\;",
            "double backslash without closing backtick"
        );
    }

    /// Terminated template literal should still get a closing backtick.
    #[test]
    fn terminated_template_preserved() {
        let output = emit("`hello`");
        assert_eq!(
            output.trim(),
            "`hello`;",
            "terminated template should have closing backtick"
        );
    }

    /// Tagged template with unterminated no-substitution template.
    #[test]
    fn tagged_unterminated_template() {
        let source = "function f(x: any) {}\nf `abc";
        let output = emit(source);
        assert!(
            output.contains("f `abc;") && !output.contains("f `abc`;"),
            "tagged unterminated template should not add closing backtick\nGot: {output}"
        );
    }
}
