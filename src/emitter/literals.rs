use super::Printer;
use crate::parser::node::Node;
use crate::transform_context::IdentifierId;

impl<'a> Printer<'a> {
    // =========================================================================
    // Literals
    // =========================================================================

    pub(super) fn emit_identifier(&mut self, node: &Node) {
        if let Some(ident) = self.arena.get_identifier(node) {
            self.write_identifier(&ident.escaped_text);
        }
    }

    pub(super) fn write_identifier_by_id(&mut self, id: IdentifierId) {
        if let Some(ident) = self.arena.identifiers.get(id as usize) {
            self.write_identifier(&ident.escaped_text);
        }
    }

    pub(super) fn emit_numeric_literal(&mut self, node: &Node) {
        if let Some(lit) = self.arena.get_literal(node) {
            self.write(&lit.text);
        }
    }

    pub(super) fn emit_string_literal(&mut self, node: &Node) {
        if let Some(lit) = self.arena.get_literal(node) {
            // Preserve original quote style from source text
            let quote = self.detect_original_quote(node).unwrap_or_else(|| {
                if self.ctx.options.single_quote {
                    '\''
                } else {
                    '"'
                }
            });
            self.write_char(quote);
            self.emit_escaped_string(&lit.text, quote);
            self.write_char(quote);
        }
    }

    /// Detect the original quote character used in source text
    fn detect_original_quote(&self, node: &Node) -> Option<char> {
        let text = self.source_text?;
        let pos = node.pos as usize;
        if pos < text.len() {
            let ch = text.as_bytes()[pos];
            if ch == b'\'' || ch == b'"' {
                return Some(ch as char);
            }
        }
        None
    }

    pub(super) fn emit_string_literal_text(&mut self, text: &str) {
        let quote = if self.ctx.options.single_quote {
            '\''
        } else {
            '"'
        };
        self.write_char(quote);
        self.emit_escaped_string(text, quote);
        self.write_char(quote);
    }

    pub(super) fn emit_escaped_string(&mut self, s: &str, quote_char: char) {
        for ch in s.chars() {
            match ch {
                '\n' => self.write("\\n"),
                '\r' => self.write("\\r"),
                '\t' => self.write("\\t"),
                '\\' => self.write("\\\\"),
                c if c == quote_char => {
                    self.write_char('\\');
                    self.write_char(c);
                }
                c => self.write_char(c),
            }
        }
    }
}
