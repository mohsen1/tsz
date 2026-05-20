use std::fmt::Write;

use crate::context::transform::IdentifierId;
use crate::emitter::Printer;
use tsz_parser::parser::node::Node;

impl<'a> Printer<'a> {
    // =========================================================================
    // Literals
    // =========================================================================

    pub(in crate::emitter) fn emit_identifier(&mut self, node: &Node) {
        let Some(ident) = self.arena.get_identifier(node) else {
            return;
        };
        let original_text = &ident.escaped_text;

        // In async function lowering (ES2015+), `arguments` inside the
        // generator body must be rewritten because the outer wrapper captures it.
        if self.ctx.rewrite_arguments_to_arguments_1 && original_text == "arguments" {
            let capture_name = self
                .ctx
                .arguments_capture_name
                .as_deref()
                .unwrap_or("arguments_1")
                .to_string();
            self.write(&capture_name);
            return;
        }

        if !self.suppress_ns_qualification
            && let Some((class_name, class_alias)) = self.scoped_class_expression_self_alias.clone()
            && original_text == class_name.as_ref()
        {
            if let Some(source_pos) = self.take_pending_source_pos() {
                self.writer
                    .write_node_with_name(class_alias.as_ref(), source_pos, original_text);
            } else {
                self.writer.write(class_alias.as_ref());
            }
            return;
        }

        // tsc preserves unicode escape sequences in identifiers verbatim.
        // When the parser detects unicode escapes (e.g., \u0041 for 'A'),
        // it stores the original source text in `original_text`. Use it
        // for emission to match tsc output.
        let emit_text = ident.original_text.as_deref().unwrap_or(original_text);

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
                self.write_identifier(emit_text);
            }
        } else if self.in_namespace_iife
            && !self.suppress_ns_qualification
            && self
                .namespace_current_class_fn_enum_names
                .contains(original_text.as_str())
        {
            // Class/function/enum declared in the CURRENT namespace block is
            // lexically in scope inside the IIFE — emit the bare identifier.
            // This must run before the `namespace_exported_names` qualifier
            // branch because the merged-prior-blocks set may include names
            // that are also in the current block, and tsc keeps same-block
            // class references unqualified (`extends PullSymbol`, not
            // `extends TypeScript.PullSymbol`).
            self.write_identifier(emit_text);
        } else if self.in_namespace_iife
            && !self.suppress_ns_qualification
            && self
                .namespace_exported_names
                .contains(original_text.as_str())
            && let Some(ref ns_name) = self.current_namespace_name
        {
            // Inside namespace IIFE, qualify namespace-object references:
            // `foo` → `ns.foo`
            let ns_name = ns_name.clone();
            self.write(&ns_name);
            self.write(".");
            self.write_identifier(emit_text);
        } else if self.in_namespace_iife
            && !self.suppress_ns_qualification
            && self
                .namespace_parent_exported_names
                .contains(original_text.as_str())
            && let Some(ref parent_name) = self.parent_namespace_name
        {
            // Inside `namespace parent.Child`, sibling namespace references
            // are qualified through the parent: `Sibling` → `parent.Sibling`.
            let parent_name = parent_name.clone();
            self.write(&parent_name);
            self.write(".");
            self.write_identifier(emit_text);
        } else if self.in_namespace_iife
            && !self.suppress_ns_qualification
            && let Some(qualifier) = self
                .namespace_ancestor_export_qualifiers
                .get(original_text.as_str())
                .cloned()
        {
            self.write(&qualifier);
            self.write(".");
            self.write_identifier(emit_text);
        } else if !self.suppress_ns_qualification
            && self
                .commonjs_exported_var_names
                .contains(original_text.as_str())
            && !self
                .commonjs_exported_var_shadow_stack
                .iter()
                .rev()
                .any(|scope| scope.contains(original_text.as_str()))
        {
            // In CJS modules, inline-exported variable references (let/const/var)
            // are rewritten to `exports.X` for both reads and writes.
            // Note: we check the set directly (not is_commonjs()) because the module
            // kind is temporarily set to None inside export statement bodies.
            self.write("exports.");
            self.write_identifier(emit_text);
        } else if !self.suppress_commonjs_named_import_substitution
            && let Some(subst) = self.commonjs_named_import_substitutions.get(original_text)
        {
            let subst = subst.clone();
            self.write(&subst);
        } else {
            self.write_identifier(emit_text);
        }
    }

    pub(in crate::emitter) fn write_identifier_by_id(&mut self, id: IdentifierId) {
        if let Some(ident) = self.arena.identifiers.get(id as usize) {
            self.write_identifier(&ident.escaped_text);
        }
    }

    pub(in crate::emitter) fn emit_bigint_literal(&mut self, node: &Node) {
        if let Some(lit) = self.arena.get_literal(node) {
            // Strip numeric separators: 1_000_000n → 1000000n
            // Only strip for targets below ES2021 — separators are valid ES2021+ syntax.
            let text = if lit.text.contains('_') && !self.ctx.options.target.supports_es2021() {
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

    pub(in crate::emitter) fn emit_numeric_literal(&mut self, node: &Node) {
        if let Some(text) = self.numeric_literal_emit_text(node) {
            self.write(&text);
        }
    }

    pub(in crate::emitter) fn numeric_literal_emit_text(&self, node: &Node) -> Option<String> {
        if let Some(lit) = self.arena.get_literal(node) {
            // Strip numeric separators: 1_000_000 → 1000000
            // Only strip for targets below ES2021 — separators are valid ES2021+ syntax.
            let had_separators = lit.text.contains('_');
            let text = if had_separators && !self.ctx.options.target.supports_es2021() {
                lit.text.chars().filter(|&c| c != '_').collect::<String>()
            } else {
                lit.text.clone()
            };

            if had_separators
                && !self.ctx.options.target.supports_es2021()
                && decimal_literal_has_exponent(&text)
                && let Ok(value) = text.parse::<f64>()
            {
                return Some(format_js_number(value));
            }

            if had_separators && !self.ctx.options.target.supports_es2021() && text.starts_with('.')
            {
                return Some(format!("0{text}"));
            }

            // Convert numeric literals that need downleveling:
            // - Binary (0b/0B) and ES2015 octal (0o/0O): only for pre-ES2015 targets
            // - Legacy octal (01, 076): for ALL targets (TSC always converts these)
            // - Any prefixed literal (0b/0o/0x) with numeric separators: for < ES2021 targets
            //   (tsc converts these to decimal because separators are an ES2021 feature)
            if let Some(converted) = self.convert_numeric_literal_downlevel(&text, had_separators) {
                return Some(converted);
            }

            if let Some(converted) =
                self.convert_decimal_separator_exponent_literal(&text, had_separators)
            {
                return Some(converted);
            }

            return Some(text);
        }
        None
    }

    fn convert_decimal_separator_exponent_literal(
        &self,
        text: &str,
        had_separators: bool,
    ) -> Option<String> {
        if !had_separators || self.ctx.options.target.supports_es2021() {
            return None;
        }
        if !text.bytes().any(|b| b == b'e' || b == b'E') {
            return None;
        }
        if text.starts_with("0b")
            || text.starts_with("0B")
            || text.starts_with("0o")
            || text.starts_with("0O")
            || text.starts_with("0x")
            || text.starts_with("0X")
        {
            return None;
        }

        text.parse::<f64>().ok().map(format_js_number)
    }

    /// Convert numeric literals that need downleveling:
    /// - Binary (0b/0B) and ES2015 octal (0o/0O): only for pre-ES2015 targets
    /// - Legacy octal (01, 076): for ALL targets
    /// - Binary/octal/hex with numeric separators: for < ES2021 targets
    ///   (numeric separators are ES2021; tsc converts prefixed literals to decimal)
    fn convert_numeric_literal_downlevel(
        &self,
        text: &str,
        had_separators: bool,
    ) -> Option<String> {
        if text.len() < 2 {
            return None;
        }
        let bytes = text.as_bytes();
        if bytes[0] != b'0' {
            return None;
        }
        let needs_es5_downlevel = !self.ctx.options.target.supports_es2015();
        // When the original literal had numeric separators (ES2021 feature),
        // tsc converts all prefixed forms (0b, 0o, 0x) to decimal for targets < ES2021.
        let needs_separator_downlevel =
            had_separators && !self.ctx.options.target.supports_es2021();
        match bytes[1] {
            b'b' | b'B' if needs_es5_downlevel || needs_separator_downlevel => {
                let digits = &text[2..];
                if digits.is_empty() {
                    return None;
                }
                let value = tsz_common::numeric::parse_radix_digits_as_f64(digits, 2)?;
                Some(format_js_number(value))
            }
            b'o' | b'O' if needs_es5_downlevel || needs_separator_downlevel => {
                let digits = &text[2..];
                if digits.is_empty() {
                    return None;
                }
                let value = tsz_common::numeric::parse_radix_digits_as_f64(digits, 8)?;
                Some(format_js_number(value))
            }
            b'x' | b'X' if needs_separator_downlevel => {
                let digits = &text[2..];
                if digits.is_empty() {
                    return None;
                }
                let value = tsz_common::numeric::parse_radix_digits_as_f64(digits, 16)?;
                Some(format_js_number(value))
            }
            b'0'..=b'9' => {
                // Legacy octal (01, 076, 009): tsc converts these for ALL targets.
                // If all digits are 0-7, parse as octal; otherwise parse as decimal.
                let digits = &text[1..]; // skip leading '0'
                let is_octal = digits.bytes().all(|b| matches!(b, b'0'..=b'7'));
                let value: f64 = if is_octal {
                    tsz_common::numeric::parse_radix_digits_as_f64(digits, 8).unwrap_or(0.0)
                } else {
                    text.parse::<f64>().unwrap_or(0.0)
                };
                Some(format_js_number(value))
            }
            _ => None,
        }
    }

    pub(in crate::emitter) fn emit_regex_literal(&mut self, node: &Node) {
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
                // Find closing / by scanning for it (handling escapes and character classes)
                let mut in_escape = false;
                let mut in_character_class = false;
                while i < max_end {
                    let ch = text.as_bytes()[i];
                    if in_escape {
                        in_escape = false;
                    } else if ch == b'\\' {
                        in_escape = true;
                    } else if ch == b'/' && !in_character_class {
                        // The parser's token span already includes trailing
                        // regex flags, including non-ASCII identifier parts.
                        self.write(&text[regex_start..max_end]);
                        return;
                    } else if ch == b'[' {
                        in_character_class = true;
                    } else if ch == b']' {
                        in_character_class = false;
                    }
                    i += 1;
                }
            }
        }
        // Fallback: use the literal text from the node
        if let Some(lit) = self.arena.get_literal(node) {
            self.write(trim_unterminated_regex_recovery_suffix(&lit.text));
        }
    }

    pub(in crate::emitter) fn emit_string_literal(&mut self, node: &Node) {
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
    pub(in crate::emitter) fn get_raw_string_literal(&self, node: &Node) -> Option<String> {
        let text = self.source_text_for_map()?;
        let bytes = text.as_bytes();
        let start = node.pos as usize;
        if start >= bytes.len() {
            return None;
        }

        let read_from_quote = |i: usize| -> Option<String> {
            let quote = bytes[i];
            let mut j = i + 1;
            let mut escaped = false;
            let mut broke_on_line_terminator = false;
            while j < bytes.len() {
                let b = bytes[j];
                if escaped {
                    escaped = false;
                    // ECMAScript LineContinuation: `\<LineTerminatorSequence>`,
                    // where the sequence may be `\n`, `\r`, or `\r\n`. Treat
                    // `\\` followed by `\r\n` as a single escaped unit so the
                    // raw-string read does not trip the line-terminator
                    // fallback branch on the trailing `\n`.
                    if b == b'\r' && bytes.get(j + 1) == Some(&b'\n') {
                        j += 2;
                    } else {
                        j += 1;
                    }
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
                    broke_on_line_terminator = true;
                    break;
                }

                j += 1;
            }

            let mut end = std::cmp::min(node.end as usize, bytes.len());
            if !broke_on_line_terminator && j >= bytes.len() {
                end = bytes.len();
            }
            if end > i {
                let mut raw = text[i..end].to_string();
                if !broke_on_line_terminator && j >= bytes.len() && raw.len() == 1 {
                    // tsc leaves a separator before the synthesized semicolon for
                    // EOF-terminated strings, even when trailing source trivia was trimmed.
                    raw.push(' ');
                }
                let mut appended_source_semicolon = false;
                if broke_on_line_terminator
                    && end < bytes.len()
                    && bytes[end] == b';'
                    && raw.ends_with(quote as char)
                {
                    raw.push(';');
                    appended_source_semicolon = true;
                }
                if broke_on_line_terminator && !appended_source_semicolon && raw.ends_with(';') {
                    raw.push(';');
                }
                Some(raw)
            } else {
                None
            }
        };

        // Skip leading trivia to find the quote. Some string-literal nodes inside
        // computed property names carry the opening `[` in their pos range.
        let mut i = start;
        while i < bytes.len() {
            match bytes[i] {
                b'\'' | b'"' => break,
                b' ' | b'\t' | b'\r' | b'\n' | b'[' => i += 1,
                _ => {
                    let scan_start = start.saturating_sub(4);
                    for q in (scan_start..start).rev() {
                        if matches!(bytes[q], b'\'' | b'"') {
                            return read_from_quote(q);
                        }
                        if !matches!(bytes[q], b' ' | b'\t' | b'\r' | b'\n' | b'[') {
                            break;
                        }
                    }
                    return None;
                }
            }
        }
        if i >= bytes.len() {
            return None;
        }

        read_from_quote(i)
    }

    pub(in crate::emitter) fn find_raw_string_literal_near(
        &self,
        node: &Node,
        literal_text: &str,
    ) -> Option<String> {
        let source = self.source_text_for_map()?;
        let bytes = source.as_bytes();
        if bytes.is_empty() {
            return None;
        }

        let approx_start = (node.pos as usize).min(bytes.len());
        let approx_end = (node.end as usize).min(bytes.len());
        let start = approx_start.saturating_sub(128);
        let end = approx_end.saturating_add(128).min(bytes.len());

        let mut i = start;
        while i < end {
            let quote = bytes[i];
            if !matches!(quote, b'\'' | b'"') {
                i += 1;
                continue;
            }

            let mut j = i + 1;
            let mut escaped = false;
            while j < end {
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
                    let raw = &source[i..=j];
                    let inner = &raw[1..raw.len() - 1];
                    if inner == literal_text {
                        return Some(raw.to_string());
                    }
                    break;
                }
                if b == b'\n' || b == b'\r' {
                    break;
                }
                j += 1;
            }

            i += 1;
        }

        None
    }

    /// Detect the original quote character used in source text.
    /// Scans forward from node.pos to skip leading trivia (whitespace/comments)
    /// and find the actual quote character.
    pub(in crate::emitter) fn detect_original_quote(&self, node: &Node) -> Option<char> {
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

    pub(in crate::emitter) fn emit_string_literal_text(&mut self, text: &str) {
        let quote = if self.ctx.options.single_quote {
            '\''
        } else {
            '"'
        };
        self.write_char(quote);
        self.emit_escaped_string(text, quote);
        self.write_char(quote);
    }

    pub(in crate::emitter) fn emit_raw_string_literal_text(&mut self, text: &str) {
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
        if quote != b'\'' && quote != b'"' {
            return None;
        }

        let quote_char = quote as char;
        let terminated = bytes[bytes.len() - 1] == quote;
        if !terminated && !raw[1..].contains("\\u{") {
            return None;
        }

        let inner_end = if terminated {
            bytes.len() - 1
        } else {
            bytes.len()
        };
        let inner = &raw[1..inner_end];
        let converted = self.downlevel_codepoint_escapes_in_literal_text(inner, quote_char, false);
        if !terminated && has_unterminated_codepoint_escape(inner) {
            return Some(format!("{quote_char}{converted}"));
        }
        Some(format!("{quote_char}{converted}{quote_char}"))
    }

    pub(in crate::emitter) fn downlevel_codepoint_escapes_in_literal_text(
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

                    if escape_invalid_codepoint_sequences {
                        out.push('\\');
                        out.push_str(&text[i..]);
                        break;
                    }
                }

                if escape_invalid_codepoint_sequences
                    && self.template_escape_sequence_is_invalid_for_string_literal(text, i)
                {
                    out.push('\\');
                    out.push('\\');
                    if let Some(ch) = text[i + 1..].chars().next() {
                        out.push(ch);
                        i += 1 + ch.len_utf8();
                    } else {
                        i += 1;
                    }
                    continue;
                }

                if i + 1 < bytes.len() {
                    // The byte after `\` may be a multi-byte UTF-8 lead byte
                    // (e.g. `\` followed by U+2028 LINE SEPARATOR which is 3
                    // bytes in UTF-8).  Decode the full character so we
                    // advance `i` past all of its bytes.
                    let after = &text[i + 1..];
                    if let Some(ch) = after.chars().next() {
                        out.push('\\');
                        out.push(ch);
                        i += 1 + ch.len_utf8();
                    } else {
                        out.push('\\');
                        i += 1;
                    }
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

    fn template_escape_sequence_is_invalid_for_string_literal(
        &self,
        text: &str,
        slash_pos: usize,
    ) -> bool {
        let bytes = text.as_bytes();
        if slash_pos + 1 >= bytes.len() {
            return false;
        }

        match bytes[slash_pos + 1] {
            b'u' => {
                if slash_pos + 2 < bytes.len() && bytes[slash_pos + 2] == b'{' {
                    return true;
                }
                slash_pos + 5 >= bytes.len()
                    || !bytes[slash_pos + 2..=slash_pos + 5]
                        .iter()
                        .all(|byte| byte.is_ascii_hexdigit())
            }
            b'x' => {
                slash_pos + 3 >= bytes.len()
                    || !bytes[slash_pos + 2..=slash_pos + 3]
                        .iter()
                        .all(|byte| byte.is_ascii_hexdigit())
            }
            b'0' => slash_pos + 2 < bytes.len() && bytes[slash_pos + 2].is_ascii_digit(),
            b'1'..=b'9' => true,
            _ => false,
        }
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

    pub(in crate::emitter) fn emit_escaped_string(&mut self, s: &str, quote_char: char) {
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
                    write!(buf, "\\u{:04X}", c as u32).expect("write to String cannot fail");
                    self.write(&buf);
                }
                c => self.write_char(c),
            }
        }
    }

    pub(in crate::emitter) fn emit_escaped_string_with_es5_surrogates(
        &mut self,
        s: &str,
        quote_char: char,
    ) {
        for ch in s.chars() {
            let cp = ch as u32;
            if cp <= 0xFFFF {
                self.emit_escaped_string(ch.encode_utf8(&mut [0; 4]), quote_char);
                continue;
            }

            let adjusted = cp - 0x10000;
            let high = 0xD800 + ((adjusted >> 10) as u16);
            let low = 0xDC00 + ((adjusted & 0x03FF) as u16);
            self.write(&format!("\\u{high:04X}\\u{low:04X}"));
        }
    }
}

/// Format an f64 value the way JavaScript's `Number.toString()` would.
/// JavaScript uses exponential notation for magnitudes >= 1e21 or < 1e-6.
fn format_js_number(value: f64) -> String {
    if value.is_infinite() {
        return "Infinity".to_string();
    }
    if value.is_nan() {
        return "NaN".to_string();
    }
    let abs = value.abs();
    if value == value.trunc() && abs < 1e21 {
        return (value as i128).to_string();
    }
    if value == 0.0 || (1e-6..1e21).contains(&abs) {
        return value.to_string();
    }

    let s = format!("{value:e}");
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

fn decimal_literal_has_exponent(text: &str) -> bool {
    let lower = text.as_bytes();
    if lower.len() >= 2
        && lower[0] == b'0'
        && matches!(lower[1], b'b' | b'B' | b'o' | b'O' | b'x' | b'X')
    {
        return false;
    }
    text.contains('e') || text.contains('E')
}

fn trim_unterminated_regex_recovery_suffix(text: &str) -> &str {
    if !text.starts_with('/') || text[1..].contains('/') {
        return text;
    }

    text.trim_end_matches(';').trim_end_matches(')')
}

fn has_unterminated_codepoint_escape(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 2 < bytes.len() {
        if bytes[i] == b'\\' && bytes[i + 1] == b'u' && bytes[i + 2] == b'{' {
            return !bytes[i + 3..].contains(&b'}');
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use crate::output::printer::{PrintOptions, Printer};
    use tsz_parser::ParserState;
    fn parse_test_source<S: Into<String>>(
        source: S,
    ) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
        let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.into());
        let root = parser.parse_source_file();
        (parser, root)
    }

    #[test]
    fn regex_literal_preserves_non_ascii_flags() {
        let source = "const 𝘳𝘦𝘨𝘦𝘹 = /(?𝘴𝘪-𝘮:^𝘧𝘰𝘰.)/𝘨𝘮𝘶;";
        let (parser, root) = parse_test_source(source);
        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;
        assert!(
            output.contains("/(?𝘴𝘪-𝘮:^𝘧𝘰𝘰.)/𝘨𝘮𝘶"),
            "Non-ASCII regex flags should be preserved.\nGot: {output}"
        );
    }

    /// Legacy octal literals (01, 076, 009) must be converted to decimal
    /// in emitted JS, matching tsc behavior for ALL targets.
    #[test]
    fn legacy_octal_converted_to_decimal() {
        let cases = [("01;", "1;"), ("076;", "62;"), ("00;", "0;"), ("07;", "7;")];
        for (source, expected_fragment) in cases {
            let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
            let root = parser.parse_source_file();
            let mut printer = Printer::new(&parser.arena, PrintOptions::default());
            printer.set_source_text(source);
            printer.print(root);
            let output = printer.finish().code;
            assert!(
                output.contains(expected_fragment),
                "Legacy octal {source} should emit {expected_fragment}\nGot: {output}"
            );
        }
    }

    /// Legacy octal with non-octal digits (08, 09, 089) are parsed as decimal
    /// by JS engines. tsc still strips the leading zero.
    #[test]
    fn legacy_octal_with_non_octal_digits() {
        let cases = [("009;", "9;"), ("08;", "8;")];
        for (source, expected_fragment) in cases {
            let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
            let root = parser.parse_source_file();
            let mut printer = Printer::new(&parser.arena, PrintOptions::default());
            printer.set_source_text(source);
            printer.print(root);
            let output = printer.finish().code;
            assert!(
                output.contains(expected_fragment),
                "Non-octal legacy form {source} should emit {expected_fragment}\nGot: {output}"
            );
        }
    }

    /// Regular decimal, hex, and float literals should NOT be modified.
    #[test]
    fn non_octal_literals_unchanged() {
        let cases = ["42;", "0;", "0.5;", "1e3;"];
        for source in cases {
            let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
            let root = parser.parse_source_file();
            let mut printer = Printer::new(&parser.arena, PrintOptions::default());
            printer.set_source_text(source);
            printer.print(root);
            let output = printer.finish().code;
            assert!(
                output.contains(source.trim_end_matches('\n')),
                "Non-octal {source} should be preserved unchanged.\nGot: {output}"
            );
        }
    }

    #[test]
    fn decimal_numeric_separators_with_exponents_downlevel_to_number_text() {
        let source = [
            "1e1_0;",
            "1e+1_0;",
            "1e-1_0;",
            "1.1e10_0;",
            "1.1e+10_0;",
            "1.1e-10_0;",
            "1_2.3_4e5_6;",
            "1_2.3_4e+5_6;",
            "1_2.3_4e-5_6;",
        ]
        .join("\n");

        let (parser, root) = parse_test_source(source);
        let mut printer = Printer::new(&parser.arena, PrintOptions::es5());
        printer.print(root);
        let output = printer.finish().code;

        for expected in [
            "10000000000;",
            "1e-10;",
            "1.1e+100;",
            "1.1e-100;",
            "1.234e+57;",
            "1.234e-55;",
        ] {
            assert!(
                output.contains(expected),
                "Expected downleveled decimal separator exponent {expected}\nGot: {output}"
            );
        }
        assert!(
            !output.contains("1e10;") && !output.contains("12.34e56;"),
            "Decimal exponent separators should be normalized through the numeric value.\nGot: {output}"
        );
    }

    #[test]
    fn unterminated_codepoint_escape_string_downlevels_to_cooked_text() {
        let source = "var x = \"\\u{00000000000067}\r\n";
        let (parser, root) = parse_test_source(source);
        let mut printer = Printer::new(&parser.arena, PrintOptions::es5());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("var x = \"g\";"),
            "ES5 should downlevel unterminated codepoint escape strings through cooked text.\nGot: {output}"
        );
    }

    #[test]
    fn incomplete_codepoint_escape_string_keeps_missing_close_quote() {
        let source = "var x = \"\\u{00000000000067";
        let (parser, root) = parse_test_source(source);
        let mut printer = Printer::new(&parser.arena, PrintOptions::es5());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("var x = \"\\u{00000000000067;"),
            "ES5 should preserve tsc's unterminated invalid codepoint escape shape.\nGot: {output}"
        );
        assert!(
            !output.contains("var x = \"\\u{00000000000067\";"),
            "ES5 should not synthesize a closing quote for incomplete codepoint escapes.\nGot: {output}"
        );
    }

    #[test]
    fn recovered_multiline_string_literals_preserve_source_semicolon_and_eof_space() {
        use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};

        let source = "var es1 = \"line 1\n\";\nvar es13 = \" \nvar es14 = \"";
        let (parser, root) = parse_test_source(source);
        let mut printer = EmitterPrinter::with_options(&parser.arena, PrinterOptions::default());
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("var es1 = \"line 1;\n\";;"),
            "Recovered multiline string literals should preserve source semicolons.\nGot: {output}"
        );
        assert!(
            output.contains("var es13 = \" ;"),
            "EOF-terminated string literal recovery should preserve trailing source text.\nGot: {output}"
        );
        assert!(
            output.contains("var es14 = \" ;"),
            "EOF-terminated string literal recovery should synthesize tsc's separator space.\nGot: {output}"
        );
    }

    #[test]
    fn unterminated_regex_in_call_does_not_duplicate_recovery_paren() {
        let source = "foo(/notregexp);";
        let (parser, root) = parse_test_source(source);
        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;
        assert!(
            output.contains("foo(/notregexp);"),
            "Unterminated regex recovery should leave the call paren to the call emitter.\nGot: {output}"
        );
        assert!(
            !output.contains("foo(/notregexp););"),
            "Unterminated regex recovery should not duplicate the call paren.\nGot: {output}"
        );
    }

    /// Unicode escape sequences in identifiers must be preserved in emitted JS,
    /// matching tsc behavior. `var \u0041 = 1;` should NOT resolve to `var A = 1;`.
    #[test]
    fn unicode_escape_in_identifier_preserved() {
        let source = "var \\u0041 = 1;";
        let (parser, root) = parse_test_source(source);
        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;
        assert!(
            output.contains("\\u0041"),
            "Unicode escape \\u0041 should be preserved in identifier.\nGot: {output}"
        );
        assert!(
            !output.starts_with("var A ="),
            "Unicode escape should NOT be resolved to 'A'.\nGot: {output}"
        );
    }

    /// When numeric literals have separators (ES2021 feature) and the target
    /// is < ES2021, tsc converts 0b/0o/0x prefixed literals to decimal.
    #[test]
    fn numeric_separator_hex_converted_to_decimal_below_es2021() {
        use tsz_common::ScriptTarget;
        // 0x00_11 → 17 (after stripping separators: 0x0011 → 17)
        let cases = [
            ("0x00_11;", "17;"),
            ("0X0_1;", "1;"),
            ("0x1100_0011;", "285212689;"),
            ("0xA0_B0_C0;", "10531008;"),
        ];
        for (source, expected) in cases {
            let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
            let root = parser.parse_source_file();
            let opts = PrintOptions {
                target: ScriptTarget::ES2015,
                ..Default::default()
            };
            let mut printer = Printer::new(&parser.arena, opts);
            printer.set_source_text(source);
            printer.print(root);
            let output = printer.finish().code;
            assert!(
                output.contains(expected),
                "Hex with separators {source} at ES2015 should emit {expected}\nGot: {output}"
            );
        }
    }

    #[test]
    fn numeric_separator_decimal_exponents_normalized_below_es2021() {
        use tsz_common::ScriptTarget;
        let source = "1e1_0\n1e+1_0\n1.1e10_0\n1_2.3_4e5_6\n1_2.3_4e-5_6";
        let (parser, root) = parse_test_source(source);
        let opts = PrintOptions {
            target: ScriptTarget::ES2020,
            ..Default::default()
        };
        let mut printer = Printer::new(&parser.arena, opts);
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        for expected in ["10000000000;", "1.1e+100;", "1.234e+57;", "1.234e-55;"] {
            assert!(
                output.contains(expected),
                "Decimal numeric separator exponent should contain {expected}\nGot: {output}"
            );
        }
    }

    #[test]
    fn numeric_separator_leading_decimal_fraction_gets_zero_prefix_below_es2021() {
        use tsz_common::ScriptTarget;
        let source = "00.5_5;\n01.5_5;\n";
        let (parser, root) = parse_test_source(source);
        let opts = PrintOptions {
            target: ScriptTarget::ES2020,
            ..Default::default()
        };
        let mut printer = Printer::new(&parser.arena, opts);
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert_eq!(
            output.matches("0.55;").count(),
            2,
            "Separator downlevel for leading decimal fractions should emit 0.55.\nGot: {output}"
        );
        assert!(
            !output.lines().any(|line| line.trim() == ".55;"),
            "Separator downlevel should not leave bare .55 fractions.\nGot: {output}"
        );
    }

    /// Octal literals with separators converted to decimal at < ES2021
    #[test]
    fn numeric_separator_octal_converted_to_decimal_below_es2021() {
        use tsz_common::ScriptTarget;
        let cases = [
            ("0o00_11;", "9;"),
            ("0O0_1;", "1;"),
            ("0o1100_0011;", "2359305;"),
        ];
        for (source, expected) in cases {
            let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
            let root = parser.parse_source_file();
            let opts = PrintOptions {
                target: ScriptTarget::ES2020,
                ..Default::default()
            };
            let mut printer = Printer::new(&parser.arena, opts);
            printer.set_source_text(source);
            printer.print(root);
            let output = printer.finish().code;
            assert!(
                output.contains(expected),
                "Octal with separators {source} at ES2020 should emit {expected}\nGot: {output}"
            );
        }
    }

    /// Binary literals with separators converted to decimal at < ES2021
    #[test]
    fn numeric_separator_binary_converted_to_decimal_below_es2021() {
        use tsz_common::ScriptTarget;
        let source = "0b1010_0001_1000_0101;";
        let expected = "41349;";
        let (parser, root) = parse_test_source(source);
        let opts = PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        };
        let mut printer = Printer::new(&parser.arena, opts);
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;
        assert!(
            output.contains(expected),
            "Binary with separators {source} at ES2015 should emit {expected}\nGot: {output}"
        );
    }

    /// Hex/octal/binary WITHOUT separators should NOT be converted at ES2015+
    /// (they are only converted at ES5 for 0b/0o syntax support)
    #[test]
    fn prefixed_literals_without_separators_unchanged_at_es2015() {
        use tsz_common::ScriptTarget;
        let cases = ["0x0011;", "0o0011;", "0b1010;"];
        for source in cases {
            let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
            let root = parser.parse_source_file();
            let opts = PrintOptions {
                target: ScriptTarget::ES2015,
                ..Default::default()
            };
            let mut printer = Printer::new(&parser.arena, opts);
            printer.set_source_text(source);
            printer.print(root);
            let output = printer.finish().code;
            assert!(
                output.contains(source.trim_end_matches('\n')),
                "Prefixed literal {source} without separators should be unchanged at ES2015.\nGot: {output}"
            );
        }
    }

    /// Unicode escape sequences in property names must be preserved.
    /// `{ \u0061: "ss" }` should NOT resolve to `{ a: "ss" }`.
    #[test]
    fn unicode_escape_in_property_name_preserved() {
        let source = "var x = { \\u0061: \"ss\" };";
        let (parser, root) = parse_test_source(source);
        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;
        assert!(
            output.contains("\\u0061"),
            "Unicode escape \\u0061 should be preserved in property name.\nGot: {output}"
        );
    }

    /// Backslash followed by a multi-byte UTF-8 character (e.g. U+2028 LINE
    /// SEPARATOR) in a string literal must not panic during downlevel emit.
    /// Previously, `downlevel_codepoint_escapes_in_literal_text` treated the
    /// byte after `\` as a single ASCII byte and advanced by 2, landing in the
    /// middle of a multi-byte character.
    #[test]
    fn backslash_followed_by_multibyte_utf8_no_panic() {
        use tsz_common::ScriptTarget;
        // U+2028 LINE SEPARATOR is 3 bytes in UTF-8: E2 80 A8
        // The source string: var x = "line 1\<LS> line 2";
        let source = "var x = \"line 1\\\u{2028} line 2\";";
        let (parser, root) = parse_test_source(source);
        let opts = PrintOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        };
        let mut printer = Printer::new(&parser.arena, opts);
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;
        // Should not panic, and should contain the string literal
        assert!(
            output.contains("line 1"),
            "Output should contain the string literal.\nGot: {output}"
        );
    }
}
