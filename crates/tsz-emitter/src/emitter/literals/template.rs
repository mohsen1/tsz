use crate::emitter::Printer;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_scanner::SyntaxKind;

/// Delimiter pair for a template part. The opening delimiter is `` ` `` for
/// templates that begin the literal (head, no-substitution) and `}` for parts
/// that resume after a substitution (middle, tail). The terminating delimiter
/// is `` ` `` for parts that close the literal (no-substitution, tail) and
/// `${` for parts that lead into another substitution (head, middle).
#[derive(Clone, Copy)]
struct TemplateDelimiters {
    open: &'static str,
    terminated_close: &'static str,
}

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

        // When the tag is a bare identifier whose CJS-named-import
        // substitution turns it into a property access (`css` →
        // `react_1.css`), wrap the substituted expression in `(0, …)` so
        // the tagged-template invocation does not bind `this` to the
        // module namespace object. Mirrors the call-expression path in
        // `expressions/call.rs` and tsc's `inlineJsxFactoryDeclarations` /
        // `jsxImportSourceNonPragmaComment` baselines.
        let cjs_subst = if !self.suppress_commonjs_named_import_substitution
            && !self.in_system_execute_body
            && let Some(tag_node) = self.arena.get(tagged.tag)
            && let Some(ident) = self.arena.get_identifier(tag_node)
        {
            self.commonjs_named_import_substitutions
                .get(&ident.escaped_text)
                .cloned()
        } else {
            None
        };

        if let Some(subst) = cjs_subst {
            self.write("(0, ");
            self.write(&subst);
            self.write(")");
        } else {
            self.emit_expression(tagged.tag);
        }

        // When the tag is `super` with type arguments (which are stripped),
        // tsc emits `super. ` to preserve the intent of a property access.
        if tagged.type_arguments.is_some()
            && let Some(tag_node) = self.arena.get(tagged.tag)
            && tag_node.kind == SyntaxKind::SuperKeyword as u16
        {
            self.write(".");
        }

        self.write_space();
        self.emit(tagged.template);
    }

    pub(in crate::emitter) fn emit_template_expression(&mut self, node: &Node) {
        let Some(tpl) = self.arena.get_template_expr(node) else {
            self.write("``");
            return;
        };

        // Emit the template head — its raw_text already carries the opening `
        // and trailing `${` so we don't synthesize delimiters here.
        self.emit(tpl.head);

        // Emit each template span (expression + middle/tail). The middle/tail
        // literal raw_text includes its own `}` and trailing `${` or closing `.
        for &span_idx in &tpl.template_spans.nodes {
            self.emit(span_idx);
        }
    }

    pub(in crate::emitter) fn emit_no_substitution_template(&mut self, node: &Node) {
        self.emit_template_part_raw(
            node,
            TemplateDelimiters {
                open: "`",
                terminated_close: "`",
            },
        );
    }

    pub(in crate::emitter) fn emit_template_span(&mut self, node: &Node) {
        let Some(span) = self.arena.get_template_span(node) else {
            return;
        };

        self.emit_template_span_leading_comments(span);
        self.emit(span.expression);
        self.emit_template_span_trailing_comments(span);
        // Emit the literal part (middle or tail). Its raw_text supplies the
        // `}` that closes the substitution and either `${` (middle) or the
        // closing ` (tail).
        self.emit(span.literal);
    }

    pub(in crate::emitter) fn emit_template_head(&mut self, node: &Node) {
        self.emit_template_part_raw(
            node,
            TemplateDelimiters {
                open: "`",
                terminated_close: "${",
            },
        );
    }

    pub(in crate::emitter) fn emit_template_middle(&mut self, node: &Node) {
        self.emit_template_part_raw(
            node,
            TemplateDelimiters {
                open: "}",
                terminated_close: "${",
            },
        );
    }

    pub(in crate::emitter) fn emit_template_tail(&mut self, node: &Node) {
        self.emit_template_part_raw(
            node,
            TemplateDelimiters {
                open: "}",
                terminated_close: "`",
            },
        );
    }

    /// Emit a template part using the parser-supplied `raw_text` recovery
    /// fact when available. The scanner stores the full token text — opening
    /// delimiter, inner content, and closing delimiter (when terminated) —
    /// in `raw_text`, so the emitter never re-scans source bytes to discover
    /// escape sequences or recover delimiter shape. When the literal is a
    /// parser-synthesized recovery sentinel (`raw_text == None`) we
    /// reconstruct the part from cooked `text` plus the expected delimiters.
    fn emit_template_part_raw(&mut self, node: &Node, delims: TemplateDelimiters) {
        if let Some(raw) = self
            .arena
            .get_literal(node)
            .and_then(|lit| lit.raw_text.as_deref())
        {
            self.write(raw);
            return;
        }

        let cooked = self
            .arena
            .get_literal(node)
            .map(|lit| lit.text.clone())
            .unwrap_or_default();
        self.write(delims.open);
        self.write(&cooked);
        if self.template_part_has_terminated_close(node, delims) {
            self.write(delims.terminated_close);
        }
    }

    /// Fallback delimiter detection when the parser did not record a raw
    /// token slice (synthetic recovery literals). We look at the node range
    /// to determine whether the source contained the expected closing
    /// delimiter.
    fn template_part_has_terminated_close(&self, node: &Node, delims: TemplateDelimiters) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };
        let close = delims.terminated_close;
        let end = node.end as usize;
        let bytes = text.as_bytes();
        if end == 0 || end > bytes.len() {
            return false;
        }
        text[..end].ends_with(close)
    }

    fn emit_template_span_leading_comments(
        &mut self,
        span: &tsz_parser::parser::node::TemplateSpanData,
    ) {
        if self.ctx.options.remove_comments {
            return;
        }

        let Some(text) = self.source_text else {
            return;
        };
        let Some(expr_node) = self.arena.get(span.expression) else {
            return;
        };
        let expr_pos = (expr_node.pos as usize).min(text.len());
        let Some(open_start) = Self::find_template_substitution_open(text, expr_pos) else {
            return;
        };
        let open_end = open_start + 2;
        let Some(gap) = text.get(open_end..expr_pos) else {
            return;
        };
        if !gap.contains("//") && !gap.contains("/*") {
            return;
        }

        let (emitted, last_comment_end, had_trailing_newline) = self.emit_comments_in_range(
            open_end as u32,
            expr_node.pos,
            false,
            gap.starts_with('\n') || gap.starts_with('\r'),
        );
        if emitted
            && !had_trailing_newline
            && let Some(trailing) = text.get(last_comment_end as usize..expr_pos)
            && trailing.bytes().any(|byte| matches!(byte, b' ' | b'\t'))
        {
            self.write_space();
        }
    }

    fn emit_template_span_trailing_comments(
        &mut self,
        span: &tsz_parser::parser::node::TemplateSpanData,
    ) {
        if self.ctx.options.remove_comments {
            return;
        }

        let Some(expr_node) = self.arena.get(span.expression) else {
            return;
        };
        let Some(lit_node) = self.arena.get(span.literal) else {
            return;
        };

        let expr_token_end = self.find_token_end_before_trivia(expr_node.pos, lit_node.pos);
        self.emit_comments_in_range(expr_token_end, lit_node.pos, true, true);
    }

    fn find_template_substitution_open(text: &str, expr_pos: usize) -> Option<usize> {
        let search = text.get(..expr_pos)?;
        let mut candidate_end = search.len();
        while let Some(candidate) = search[..candidate_end].rfind("${") {
            if Self::is_template_substitution_trivia(text, candidate + 2, expr_pos) {
                return Some(candidate);
            }
            candidate_end = candidate;
        }
        None
    }

    fn is_template_substitution_trivia(text: &str, start: usize, end: usize) -> bool {
        let bytes = text.as_bytes();
        let mut pos = start;
        while pos < end {
            match bytes[pos] {
                b' ' | b'\t' | b'\r' | b'\n' => pos += 1,
                b'/' if pos + 1 < end && bytes[pos + 1] == b'/' => {
                    pos += 2;
                    while pos < end && !matches!(bytes[pos], b'\r' | b'\n') {
                        pos += 1;
                    }
                }
                b'/' if pos + 1 < end && bytes[pos + 1] == b'*' => {
                    pos += 2;
                    let Some(close_rel) = text.get(pos..end).and_then(|tail| tail.find("*/"))
                    else {
                        return false;
                    };
                    pos += close_rel + 2;
                }
                _ => return false,
            }
        }
        true
    }

    /// Inner raw text of a template part — the bytes between the opening and
    /// closing delimiters as they appeared in source. Reads the
    /// parser-supplied `raw_text` recovery fact and strips the delimiters that
    /// the scanner included in the token slice. Falls back to the cooked text
    /// when the parser did not record a raw slice (synthetic recovery).
    pub(in crate::emitter) fn get_raw_template_part_text(&self, node: &Node) -> Option<String> {
        let lit = self.arena.get_literal(node)?;
        if let Some(raw) = lit.raw_text.as_deref() {
            return Some(strip_template_delimiters(node.kind, raw).to_string());
        }
        Some(lit.text.clone())
    }
}

/// Strip the opening and (when present) closing delimiters from a template
/// part's raw source slice. The scanner stores the full token text, so a
/// terminated `NoSubstitutionTemplateLiteral` is `` `xxx` ``, a terminated
/// `TemplateHead` is `` `xxx${ ``, a terminated `TemplateMiddle` is `}xxx${`,
/// and a terminated `TemplateTail` is `}xxx``. Unterminated parts are missing
/// their trailing delimiter, which `strip_suffix` reports by returning `None`
/// — we preserve the rest of the slice verbatim in that case.
fn strip_template_delimiters(kind: u16, raw: &str) -> &str {
    let (open, close): (&str, &str) = if kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16 {
        ("`", "`")
    } else if kind == SyntaxKind::TemplateHead as u16 {
        ("`", "${")
    } else if kind == SyntaxKind::TemplateMiddle as u16 {
        ("}", "${")
    } else if kind == SyntaxKind::TemplateTail as u16 {
        ("}", "`")
    } else {
        return raw;
    };
    let inner = raw.strip_prefix(open).unwrap_or(raw);
    inner.strip_suffix(close).unwrap_or(inner)
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

    #[test]
    fn template_span_comments_stay_inside_substitution() {
        let output = emit(
            "`head${ // single line comment\n10\n}\nmiddle${\n/* Multi-\n * line\n */\n 20\n // closing comment\n}\ntail`;",
        );

        assert!(
            output.contains("`head${ // single line comment\n10}\n"),
            "Line comment after template substitution open should stay on the `${{` line.\nGot: {output}"
        );
        assert!(
            output.contains("20\n// closing comment\n}\ntail`;"),
            "Trailing comments before template substitution close should stay before `}}`.\nGot: {output}"
        );
    }

    #[test]
    fn invalid_no_substitution_template_statement_does_not_duplicate_semicolon() {
        let output = emit(
            r"`\u`;
`\x0`;
",
        );
        assert_eq!(
            output, "`\\u`;\n`\\x0`;\n",
            "Invalid no-substitution template statements should use the source statement semicolon once.\nGot: {output}"
        );
    }

    #[test]
    fn invalid_template_expression_statement_does_not_duplicate_semicolon() {
        let output = emit(
            r"`\u${0}`;
`${0}\x`;
",
        );
        assert_eq!(
            output, "`\\u${0}`;\n`${0}\\x`;\n",
            "Invalid template expression statements should not synthesize an extra empty statement.\nGot: {output}"
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
