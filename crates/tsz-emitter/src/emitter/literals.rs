use std::fmt::Write;

use super::Printer;
use crate::transform_context::IdentifierId;
use tsz_parser::parser::node::Node;

impl<'a> Printer<'a> {
    // =========================================================================
    // Literals
    // =========================================================================

    pub(super) fn emit_identifier(&mut self, node: &Node) {
        if let Some(ident) = self.arena.get_identifier(node) {
            let original_text = &ident.escaped_text;

            // Check if this variable has been renamed for block scoping (ES5 for-of shadowing)
            if let Some(renamed) = self.ctx.block_scope_state.get_emitted_name(original_text) {
                self.write(&renamed);
            } else {
                self.write_identifier(original_text);
            }
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

    pub(super) fn emit_regex_literal(&mut self, node: &Node) {
        // Regex literals should be emitted exactly as they appear in source
        // to preserve the pattern and flags (e.g., /\r\n/g)
        if let Some(text) = self.source_text {
            let start = node.pos as usize;
            let max_end = std::cmp::min(node.end as usize, text.len());
            // Skip leading trivia to find the start of the regex
            let mut i = start;
            while i < max_end && matches!(text.as_bytes()[i], b' ' | b'\t' | b'\r' | b'\n') {
                i += 1;
            }
            if i < max_end && text.as_bytes()[i] == b'/' {
                let regex_start = i;
                i += 1; // Skip opening /
                // Find closing / by scanning for it (handling escapes)
                let mut escaped = false;
                while i < max_end {
                    match text.as_bytes()[i] {
                        b'\\' if !escaped => escaped = true,
                        b'/' if !escaped => {
                            i += 1; // Include closing /
                            // Include any flags (g, i, m, etc.)
                            while i < max_end
                                && matches!(text.as_bytes()[i], b'a'..=b'z' | b'A'..=b'Z')
                            {
                                i += 1;
                            }
                            self.write(&text[regex_start..i]);
                            return;
                        }
                        _ => escaped = false,
                    }
                    i += 1;
                }
            }
        }
        // Fallback: use the literal text from the node
        if let Some(lit) = self.arena.get_literal(node) {
            self.write(&lit.text);
        }
    }

    pub(super) fn emit_string_literal(&mut self, node: &Node) {
        // Try to use raw source text to preserve line continuations etc.
        if let Some(raw) = self.get_raw_string_literal(node) {
            self.write(&raw);
            return;
        }
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

    /// Get the raw string literal from source text, preserving line continuations.
    fn get_raw_string_literal(&self, node: &Node) -> Option<String> {
        let text = self.source_text?;
        let bytes = text.as_bytes();
        let start = node.pos as usize;
        let end = std::cmp::min(node.end as usize, bytes.len());
        // Skip leading trivia to find the quote
        let mut i = start;
        while i < end {
            match bytes[i] {
                b'\'' | b'"' => break,
                b' ' | b'\t' | b'\r' | b'\n' => i += 1,
                _ => return None,
            }
        }
        if i >= end {
            return None;
        }
        Some(text[i..end].to_string())
    }

    /// Detect the original quote character used in source text.
    /// Scans forward from node.pos to skip leading trivia (whitespace/comments)
    /// and find the actual quote character.
    fn detect_original_quote(&self, node: &Node) -> Option<char> {
        let text = self.source_text?;
        let bytes = text.as_bytes();
        let start = node.pos as usize;
        let end = std::cmp::min(node.end as usize, bytes.len());
        for i in start..end {
            match bytes[i] {
                b'\'' => return Some('\''),
                b'"' => return Some('"'),
                b' ' | b'\t' | b'\r' | b'\n' => continue,
                _ => break,
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
                '\0' => self.write("\\0"),
                c if c == quote_char => {
                    self.write_char('\\');
                    self.write_char(c);
                }
                c if (c as u32) < 0x20 || c == '\x7F' => {
                    let mut buf = String::new();
                    write!(buf, "\\u{:04X}", c as u32).unwrap();
                    self.write(&buf);
                }
                c => self.write_char(c),
            }
        }
    }
}
