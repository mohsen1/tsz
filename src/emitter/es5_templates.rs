use super::{TemplateParts, Printer};
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::parser::node::{TaggedTemplateData, TemplateExprData, Node};
use crate::scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(super) fn emit_template_literal_es5(&mut self, node: &Node, idx: NodeIndex) -> bool {
        match node.kind {
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    self.emit_string_literal_text(&lit.text);
                    return true;
                }
                false
            }
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                if let Some(tpl) = self.arena.get_template_expr(node) {
                    self.emit_template_expression_es5(tpl);
                    return true;
                }
                false
            }
            k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => {
                if let Some(tagged) = self.arena.get_tagged_template(node) {
                    self.emit_tagged_template_expression_es5(tagged, idx);
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    fn emit_tagged_template_expression_es5(&mut self, tagged: &TaggedTemplateData, idx: NodeIndex) {
        let Some(parts) = self.collect_template_parts(tagged.template) else {
            self.emit_expression(tagged.tag);
            self.emit(tagged.template);
            return;
        };

        let temp_var = self.tagged_template_var_name(idx);

        self.emit_expression(tagged.tag);
        self.write("(");
        self.write(&temp_var);
        self.write(" || (");
        self.write(&temp_var);
        self.write(" = __makeTemplateObject(");
        self.emit_string_array_literal(&parts.cooked);
        self.write(", ");
        self.emit_string_array_literal(&parts.raw);
        self.write("))");
        for expr in parts.expressions {
            self.write(", ");
            self.emit_expression(expr);
        }
        self.write(")");
    }

    fn emit_template_expression_es5(&mut self, tpl: &TemplateExprData) {
        let head_text = self
            .arena
            .get(tpl.head)
            .and_then(|node| self.arena.get_literal(node))
            .map(|lit| lit.text.as_str())
            .unwrap_or("");

        self.write("(");
        self.emit_string_literal_text(head_text);

        for &span_idx in &tpl.template_spans.nodes {
            let Some(span_node) = self.arena.get(span_idx) else {
                continue;
            };
            let Some(span) = self.arena.get_template_span(span_node) else {
                continue;
            };

            self.write(" + ");
            self.write("(");
            self.emit_expression(span.expression);
            self.write(")");

            let literal_text = self
                .arena
                .get(span.literal)
                .and_then(|node| self.arena.get_literal(node))
                .map(|lit| lit.text.as_str())
                .unwrap_or("");
            self.write(" + ");
            self.emit_string_literal_text(literal_text);
        }

        self.write(")");
    }

    fn emit_string_array_literal(&mut self, parts: &[String]) {
        let quote = if self.ctx.options.single_quote {
            '\''
        } else {
            '"'
        };
        self.write("[");
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.write_char(quote);
            self.emit_escaped_string(part, quote);
            self.write_char(quote);
        }
        self.write("]");
    }

    fn collect_template_parts(&self, template_idx: NodeIndex) -> Option<TemplateParts> {
        let node = self.arena.get(template_idx)?;
        match node.kind {
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => {
                let cooked = self
                    .arena
                    .get_literal(node)
                    .map(|lit| lit.text.clone())
                    .unwrap_or_default();
                let raw = self.template_raw_text(node, &cooked);
                Some(TemplateParts {
                    cooked: vec![cooked],
                    raw: vec![raw],
                    expressions: Vec::new(),
                })
            }
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                let tpl = self.arena.get_template_expr(node)?;
                let mut cooked = Vec::with_capacity(tpl.template_spans.nodes.len() + 1);
                let mut raw = Vec::with_capacity(tpl.template_spans.nodes.len() + 1);
                let mut expressions = Vec::with_capacity(tpl.template_spans.nodes.len());

                let head_node = self.arena.get(tpl.head)?;
                let head_text = self
                    .arena
                    .get_literal(head_node)
                    .map(|lit| lit.text.clone())
                    .unwrap_or_default();
                let head_raw = self.template_raw_text(head_node, &head_text);
                cooked.push(head_text);
                raw.push(head_raw);

                for &span_idx in &tpl.template_spans.nodes {
                    let span_node = self.arena.get(span_idx)?;
                    let span = self.arena.get_template_span(span_node)?;
                    expressions.push(span.expression);

                    let literal_node = self.arena.get(span.literal)?;
                    let literal_text = self
                        .arena
                        .get_literal(literal_node)
                        .map(|lit| lit.text.clone())
                        .unwrap_or_default();
                    let literal_raw = self.template_raw_text(literal_node, &literal_text);
                    cooked.push(literal_text);
                    raw.push(literal_raw);
                }

                Some(TemplateParts {
                    cooked,
                    raw,
                    expressions,
                })
            }
            _ => None,
        }
    }

    fn template_raw_text(&self, node: &Node, cooked_fallback: &str) -> String {
        let Some(text) = self.source_text else {
            return cooked_fallback.to_string();
        };

        let (skip_leading, allow_dollar_brace, allow_backtick) = match node.kind {
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => (1_usize, false, true),
            k if k == SyntaxKind::TemplateHead as u16 => (1_usize, true, true),
            k if k == SyntaxKind::TemplateMiddle as u16 => (1_usize, true, true),
            k if k == SyntaxKind::TemplateTail as u16 => (1_usize, false, true),
            _ => return cooked_fallback.to_string(),
        };

        let start = node.pos as usize;
        if start >= text.len() {
            return cooked_fallback.to_string();
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
                return text[start + skip_leading..i].to_string();
            }

            if allow_dollar_brace && ch == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                return text[start + skip_leading..i].to_string();
            }

            i += 1;
        }

        cooked_fallback.to_string()
    }
}
