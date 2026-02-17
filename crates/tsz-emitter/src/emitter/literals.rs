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
                // Use write_identifier so source map name recording still works.
                // When renamed differs from original, the source map records the original
                // name so debuggers can map back to the source.
                if renamed != *original_text {
                    if let Some(source_pos) = self.take_pending_source_pos() {
                        self.writer
                            .write_node_with_name(&renamed, source_pos, original_text);
                    } else {
                        self.writer.write(&renamed);
                    }
                } else {
                    self.write_identifier(original_text);
                }
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
            // Strip numeric separators: 1_000_000 → 1000000
            let text = if lit.text.contains('_') {
                lit.text.chars().filter(|&c| c != '_').collect::<String>()
            } else {
                lit.text.clone()
            };

            // Convert numeric literals that need downleveling:
            // - Binary (0b/0B) and ES2015 octal (0o/0O): only for pre-ES2015 targets
            // - Legacy octal (01, 076): for ALL targets (TSC always converts these)
            if let Some(converted) = self.convert_numeric_literal_downlevel(&text) {
                self.write(&converted);
                return;
            }

            self.write(&text);
        }
    }

    /// Convert numeric literals that need downleveling:
    /// - Binary (0b/0B) and ES2015 octal (0o/0O): only for pre-ES2015 targets
    /// - Legacy octal (01, 076): for ALL targets
    fn convert_numeric_literal_downlevel(&self, text: &str) -> Option<String> {
        if text.len() < 2 {
            return None;
        }
        let bytes = text.as_bytes();
        if bytes[0] != b'0' {
            return None;
        }
        let needs_es5_downlevel = !self.ctx.options.target.supports_es2015();
        match bytes[1] {
            b'b' | b'B' if needs_es5_downlevel => {
                // Binary literal: parse and convert to decimal (or scientific notation for large values)
                let digits = &text[2..];
                if digits.is_empty() {
                    return None;
                }
                // Parse as f64 to handle overflow to Infinity correctly
                let value: f64 = u128::from_str_radix(digits, 2)
                    .map(|v| v as f64)
                    .unwrap_or_else(|_| {
                        // For very large binary numbers, compute as f64 directly
                        digits
                            .bytes()
                            .fold(0.0_f64, |acc, b| acc * 2.0 + (b - b'0') as f64)
                    });
                Some(format_js_number(value))
            }
            b'o' | b'O' if needs_es5_downlevel => {
                // Octal literal: parse and convert to decimal
                let digits = &text[2..];
                if digits.is_empty() {
                    return None;
                }
                let value: f64 = u128::from_str_radix(digits, 8)
                    .map(|v| v as f64)
                    .unwrap_or_else(|_| {
                        digits
                            .bytes()
                            .fold(0.0_f64, |acc, b| acc * 8.0 + (b - b'0') as f64)
                    });
                Some(format_js_number(value))
            }
            _ => None,
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
                            while i < max_end && text.as_bytes()[i].is_ascii_alphabetic() {
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
            let quote = self.detect_original_quote(node).unwrap_or({
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
        for &byte in bytes.iter().take(end).skip(start) {
            match byte {
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

/// Format an f64 value the way JavaScript's `Number.toString()` would.
/// JavaScript uses exponential notation for integers >= 1e21.
fn format_js_number(value: f64) -> String {
    if value.is_infinite() {
        return "Infinity".to_string();
    }
    if value.is_nan() {
        return "NaN".to_string();
    }
    // For integers < 1e21, emit as plain integer (no decimal point)
    // JavaScript switches to exponential notation at 1e21
    if value == value.trunc() && value.abs() < 1e21 {
        return format!("{}", value as i128);
    }
    // For large values or non-integers, use JavaScript-style formatting
    // JavaScript's Number.toString() uses exponential for >= 1e21
    // Format: significant digits + e+exponent
    let s = format!("{:e}", value);
    // Rust's {:e} produces lowercase 'e' like "9.671406556917009e24"
    // JS uses "9.671406556917009e+24" (with explicit + sign)
    if let Some(pos) = s.find('e') {
        let (mantissa, exp_part) = s.split_at(pos);
        let exp_str = &exp_part[1..]; // skip 'e'
        if !exp_str.starts_with('-') && !exp_str.starts_with('+') {
            return format!("{}e+{}", mantissa, exp_str);
        }
        return s;
    }
    s
}
