use super::super::{Printer, TemplateParts};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{Node, TaggedTemplateData, TemplateExprData};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn emit_template_literal_es5(
        &mut self,
        node: &Node,
        idx: NodeIndex,
    ) -> bool {
        match node.kind {
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => {
                if self.arena.get_literal(node).is_none() && self.source_text.is_none() {
                    return false;
                }

                let raw_text = self
                    .get_raw_template_part_text(node)
                    .or_else(|| self.arena.get_literal(node).map(|lit| lit.text.clone()))
                    .unwrap_or_default();
                let quote = if self.ctx.options.single_quote {
                    '\''
                } else {
                    '"'
                };
                let downleveled =
                    self.downlevel_codepoint_escapes_in_literal_text(&raw_text, quote, true);
                self.emit_raw_string_literal_text(&downleveled);
                true
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

        self.emit_expression(tagged.tag);
        self.write("(");

        if self.ctx.file_is_module {
            let temp_var = self.tagged_template_var_name(idx);
            self.write(&temp_var);
            self.write(" || (");
            self.write(&temp_var);
            self.write(" = ");
            self.write_helper("__makeTemplateObject");
            self.write("(");
            self.emit_cooked_array_literal(&parts);
            self.write(", ");
            self.emit_string_array_literal(&parts.raw);
            self.write("))");
        } else {
            self.write_helper("__makeTemplateObject");
            self.write("(");
            self.emit_cooked_array_literal(&parts);
            self.write(", ");
            self.emit_string_array_literal(&parts.raw);
            self.write(")");
        }

        for expr in parts.expressions {
            self.write(", ");
            self.emit_expression(expr);
        }
        self.write(")");
    }

    fn emit_template_expression_es5(&mut self, tpl: &TemplateExprData) {
        let quote = if self.ctx.options.single_quote {
            '\''
        } else {
            '"'
        };
        let head_text = self
            .arena
            .get(tpl.head)
            .and_then(|node| self.get_raw_template_part_text(node))
            .map(|raw| self.downlevel_codepoint_escapes_in_literal_text(&raw, quote, true))
            .unwrap_or_default();

        // TypeScript 5.x uses .concat() for template literal downleveling:
        // `hello ${name} world` → "hello ".concat(name, " world")
        self.emit_raw_string_literal_text(&head_text);

        for &span_idx in &tpl.template_spans.nodes {
            let Some(span_node) = self.arena.get(span_idx) else {
                continue;
            };
            let Some(span) = self.arena.get_template_span(span_node) else {
                continue;
            };

            let literal_text = self
                .arena
                .get(span.literal)
                .and_then(|node| self.get_raw_template_part_text(node))
                .map(|raw| self.downlevel_codepoint_escapes_in_literal_text(&raw, quote, true))
                .unwrap_or_default();

            self.write(".concat(");
            self.emit_expression(span.expression);
            if !literal_text.is_empty() {
                self.write(", ");
                self.emit_raw_string_literal_text(&literal_text);
            }
            self.write(")");
        }
    }

    fn emit_cooked_array_literal(&mut self, parts: &TemplateParts) {
        let quote = if self.ctx.options.single_quote {
            '\''
        } else {
            '"'
        };
        self.write("[");
        for (i, (text, &is_invalid)) in parts
            .cooked
            .iter()
            .zip(parts.cooked_invalid.iter())
            .enumerate()
        {
            if i > 0 {
                self.write(", ");
            }
            if is_invalid {
                self.write("void 0");
            } else {
                self.write_char(quote);
                self.emit_escaped_string(text, quote);
                self.write_char(quote);
            }
        }
        self.write("]");
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
                let lit = self.arena.get_literal(node);
                let cooked = lit.map(|l| l.text.clone()).unwrap_or_default();
                let has_invalid = lit.is_some_and(|l| l.has_invalid_escape);
                let raw = self
                    .get_raw_template_part_text(node)
                    .unwrap_or_else(|| cooked.clone());
                Some(TemplateParts {
                    cooked: vec![cooked],
                    cooked_invalid: vec![has_invalid],
                    raw: vec![raw],
                    expressions: Vec::new(),
                })
            }
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                let tpl = self.arena.get_template_expr(node)?;
                let mut cooked = Vec::with_capacity(tpl.template_spans.nodes.len() + 1);
                let mut cooked_invalid = Vec::with_capacity(tpl.template_spans.nodes.len() + 1);
                let mut raw = Vec::with_capacity(tpl.template_spans.nodes.len() + 1);
                let mut expressions = Vec::with_capacity(tpl.template_spans.nodes.len());

                let head_node = self.arena.get(tpl.head)?;
                let head_lit = self.arena.get_literal(head_node);
                let head_text = head_lit.map(|l| l.text.clone()).unwrap_or_default();
                let head_invalid = head_lit.is_some_and(|l| l.has_invalid_escape);
                let head_raw = self
                    .get_raw_template_part_text(head_node)
                    .unwrap_or_else(|| head_text.clone());
                cooked.push(head_text);
                cooked_invalid.push(head_invalid);
                raw.push(head_raw);

                for &span_idx in &tpl.template_spans.nodes {
                    let span_node = self.arena.get(span_idx)?;
                    let span = self.arena.get_template_span(span_node)?;
                    expressions.push(span.expression);

                    let literal_node = self.arena.get(span.literal)?;
                    let literal_lit = self.arena.get_literal(literal_node);
                    let literal_text = literal_lit.map(|l| l.text.clone()).unwrap_or_default();
                    let literal_invalid = literal_lit.is_some_and(|l| l.has_invalid_escape);
                    let literal_raw = self
                        .get_raw_template_part_text(literal_node)
                        .unwrap_or_else(|| literal_text.clone());
                    cooked.push(literal_text);
                    cooked_invalid.push(literal_invalid);
                    raw.push(literal_raw);
                }

                Some(TemplateParts {
                    cooked,
                    cooked_invalid,
                    raw,
                    expressions,
                })
            }
            _ => None,
        }
    }
}
