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
            } else if self.in_namespace_iife
                && !self.suppress_ns_qualification
                && self
                    .namespace_exported_names
                    .contains(original_text.as_str())
                && let Some(ref ns_name) = self.current_namespace_name
            {
                // Inside namespace IIFE, qualify exported variable references:
                // `foo` → `ns.foo`
                let ns_name = ns_name.clone();
                self.write(&ns_name);
                self.write(".");
                self.write_identifier(original_text);
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

    pub(super) fn emit_bigint_literal(&mut self, node: &Node) {
        if let Some(lit) = self.arena.get_literal(node) {
            // Strip numeric separators: 1_000_000n → 1000000n
            let text = if lit.text.contains('_') {
                lit.text.chars().filter(|&c| c != '_').collect::<String>()
            } else {
                lit.text.clone()
            };

            // TSC converts binary/octal BigInt literals to decimal form,
            // and lowercases hex BigInt literals.
            if let Some(converted) = Self::convert_bigint_literal(&text) {
                self.write(&converted);
            } else {
                self.write(&text);
            }
        }
    }

    /// Convert `BigInt` literals with non-decimal bases:
    /// - Binary (0b101n) → decimal (5n)
    /// - Octal (0o567n) → decimal (375n)
    /// - Hex (0xC0Bn) → lowercase hex (0xc0bn)
    fn convert_bigint_literal(text: &str) -> Option<String> {
        // Must end with 'n' and start with '0'
        if text.len() < 3 || !text.ends_with('n') || !text.starts_with('0') {
            return None;
        }
        let without_n = &text[..text.len() - 1]; // Strip trailing 'n'
        let bytes = without_n.as_bytes();
        if bytes.len() < 2 {
            return None;
        }
        match bytes[1] {
            b'b' | b'B' => {
                // Binary → decimal
                let digits = &without_n[2..];
                if digits.is_empty() {
                    return None;
                }
                let value = u128::from_str_radix(digits, 2).ok()?;
                Some(format!("{value}n"))
            }
            b'o' | b'O' => {
                // Octal → decimal
                let digits = &without_n[2..];
                if digits.is_empty() {
                    return None;
                }
                let value = u128::from_str_radix(digits, 8).ok()?;
                Some(format!("{value}n"))
            }
            b'x' | b'X' => {
                // Hex → lowercase hex
                let lowered = without_n.to_lowercase();
                if lowered == *without_n {
                    return None;
                }
                Some(format!("{lowered}n"))
            }
            _ => None,
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
        // Try to use raw source text to preserve line continuations and escape forms.
        if let Some(raw) = self.get_raw_string_literal(node) {
            if !self.ctx.options.target.supports_es2015()
                && let Some(downleveled) = self.downlevel_string_literal_for_es5(&raw)
            {
                self.write(&downleveled);
                return;
            }
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
        if start >= bytes.len() {
            return None;
        }

        // Skip leading trivia to find the quote
        let mut i = start;
        while i < bytes.len() {
            match bytes[i] {
                b'\'' | b'"' => break,
                b' ' | b'\t' | b'\r' | b'\n' => i += 1,
                _ => return None,
            }
        }
        if i >= bytes.len() {
            return None;
        }

        let quote = bytes[i];
        let mut j = i + 1;
        let mut escaped = false;
        while j < bytes.len() {
            let b = bytes[j];
            if escaped {
                escaped = false;
                j += 1;
                continue;
            }

            if b == b'\\' {
                escaped = true;
                j += 1;
                continue;
            }

            if b == quote {
                return Some(text[i..=j].to_string());
            }

            // Unterminated literal fallback: use parser end range.
            if b == b'\n' || b == b'\r' {
                break;
            }

            j += 1;
        }

        let end = std::cmp::min(node.end as usize, bytes.len());
        if end > i {
            Some(text[i..end].to_string())
        } else {
            None
        }
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

    pub(super) fn emit_raw_string_literal_text(&mut self, text: &str) {
        let quote = if self.ctx.options.single_quote {
            '\''
        } else {
            '"'
        };
        self.write_char(quote);
        self.write(text);
        self.write_char(quote);
    }

    fn downlevel_string_literal_for_es5(&self, raw: &str) -> Option<String> {
        let bytes = raw.as_bytes();
        if bytes.len() < 2 {
            return None;
        }

        let quote = bytes[0];
        if (quote != b'\'' && quote != b'"') || bytes[bytes.len() - 1] != quote {
            return None;
        }

        let quote_char = quote as char;
        let inner = &raw[1..bytes.len() - 1];
        let converted = self.downlevel_codepoint_escapes_in_literal_text(inner, quote_char, false);
        Some(format!("{quote_char}{converted}{quote_char}"))
    }

    pub(super) fn downlevel_codepoint_escapes_in_literal_text(
        &self,
        text: &str,
        quote_char: char,
        escape_invalid_codepoint_sequences: bool,
    ) -> String {
        let bytes = text.as_bytes();
        let mut out = String::new();
        let mut i = 0;

        while i < bytes.len() {
            if bytes[i] == b'\\' {
                if i + 2 < bytes.len() && bytes[i + 1] == b'u' && bytes[i + 2] == b'{' {
                    let mut j = i + 3;
                    while j < bytes.len() && bytes[j] != b'}' {
                        j += 1;
                    }
                    if j < bytes.len() {
                        let hex = &text[i + 3..j];
                        let valid_hex =
                            !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit());
                        if valid_hex
                            && let Ok(cp) = u32::from_str_radix(hex, 16)
                            && cp <= 0x10FFFF
                        {
                            self.push_downleveled_codepoint(&mut out, cp, quote_char);
                            i = j + 1;
                            continue;
                        }

                        // Invalid \u{...} sequence handling differs by context:
                        // - string literals: keep `\u{...}` as-is
                        // - template downlevel to string: emit literal backslash (`\\u{...}`)
                        if escape_invalid_codepoint_sequences {
                            out.push('\\');
                        }
                        out.push_str(&text[i..=j]);
                        i = j + 1;
                        continue;
                    }
                }

                if i + 1 < bytes.len() {
                    out.push('\\');
                    out.push(bytes[i + 1] as char);
                    i += 2;
                } else {
                    out.push('\\');
                    i += 1;
                }
                continue;
            }

            let ch = match text[i..].chars().next() {
                Some(ch) => ch,
                None => break,
            };
            if ch == quote_char {
                out.push('\\');
                out.push(ch);
            } else if ch == '\\' {
                out.push_str("\\\\");
            } else {
                out.push(ch);
            }
            i += ch.len_utf8();
        }

        out
    }

    fn push_downleveled_codepoint(&self, out: &mut String, cp: u32, quote_char: char) {
        if cp == 0 {
            out.push_str("\\0");
            return;
        }

        if cp <= 0xFFFF {
            self.push_downleveled_code_unit(out, cp as u16, quote_char);
            return;
        }

        let adjusted = cp - 0x10000;
        let high = 0xD800 + ((adjusted >> 10) as u16);
        let low = 0xDC00 + ((adjusted & 0x03FF) as u16);
        self.push_downleveled_code_unit(out, high, quote_char);
        self.push_downleveled_code_unit(out, low, quote_char);
    }

    fn push_downleveled_code_unit(&self, out: &mut String, code_unit: u16, quote_char: char) {
        if code_unit as u32 == quote_char as u32 {
            out.push('\\');
            out.push(quote_char);
            return;
        }

        match code_unit {
            0x0008 => out.push_str("\\b"),
            0x0009 => out.push_str("\\t"),
            0x000A => out.push_str("\\n"),
            0x000C => out.push_str("\\f"),
            0x000D => out.push_str("\\r"),
            0x005C => out.push_str("\\\\"),
            0x0020..=0x007E => {
                out.push(code_unit as u8 as char);
            }
            _ => {
                let _ = write!(out, "\\u{code_unit:04X}");
            }
        }
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
    let s = format!("{value:e}");
    // Rust's {:e} produces lowercase 'e' like "9.671406556917009e24"
    // JS uses "9.671406556917009e+24" (with explicit + sign)
    if let Some(pos) = s.find('e') {
        let (mantissa, exp_part) = s.split_at(pos);
        let exp_str = &exp_part[1..]; // skip 'e'
        if !exp_str.starts_with('-') && !exp_str.starts_with('+') {
            return format!("{mantissa}e+{exp_str}");
        }
        return s;
    }
    s
}
