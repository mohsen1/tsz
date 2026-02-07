use super::Printer;
use crate::printer::safe_slice;
use crate::source_writer::SourcePosition;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    fn take_pending_source_pos(&mut self) -> Option<SourcePosition> {
        self.pending_source_pos.take()
    }

    // =========================================================================
    // Safe String Slicing Helpers
    // =========================================================================

    /// Safely get a slice of text, returning empty string if out of bounds.
    /// This prevents panics from invalid string indices.
    ///
    /// Used for safe extraction of source text segments during emission,
    /// particularly for comment extraction and source mapping.
    #[allow(dead_code)] // Available for future use in comment extraction
    pub(super) fn safe_slice_text<'b>(&self, text: &'b str, start: u32, end: u32) -> &'b str {
        safe_slice::slice(text, start as usize, end as usize)
    }

    // =========================================================================
    // Output Helpers (delegate to SourceWriter)
    // pub(super) for access from submodules (expressions, statements, declarations)
    // =========================================================================

    /// Write text to output.
    pub(super) fn write(&mut self, text: &str) {
        if let Some(source_pos) = self.take_pending_source_pos() {
            self.writer.write_node(text, source_pos);
        } else {
            self.writer.write(text);
        }
    }

    /// Write identifier text to output with name mapping when available.
    pub(super) fn write_identifier(&mut self, text: &str) {
        if let Some(source_pos) = self.take_pending_source_pos() {
            self.writer.write_node_with_name(text, source_pos, text);
        } else {
            self.writer.write(text);
        }
    }

    /// Write a single character.
    pub(super) fn write_char(&mut self, ch: char) {
        if let Some(source_pos) = self.take_pending_source_pos() {
            let mut buf = [0u8; 4];
            let text = ch.encode_utf8(&mut buf);
            self.writer.write_node(text, source_pos);
        } else {
            self.writer.write_char(ch);
        }
    }

    /// Write a newline.
    pub(super) fn write_line(&mut self) {
        self.writer.write_line();
    }

    /// Write a space.
    pub(super) fn write_space(&mut self) {
        self.writer.write_space();
    }

    /// Write an unsigned integer.
    pub(super) fn write_usize(&mut self, value: usize) {
        if let Some(source_pos) = self.take_pending_source_pos() {
            self.writer.write_node_usize(value, source_pos);
        } else {
            self.writer.write_usize(value);
        }
    }

    /// Write a semicolon (respecting options).
    pub(super) fn write_semicolon(&mut self) {
        if !self.ctx.options.omit_trailing_semicolon {
            self.write(";");
        }
    }

    /// Increase indentation.
    pub(super) fn increase_indent(&mut self) {
        self.writer.increase_indent();
    }

    /// Decrease indentation.
    pub(super) fn decrease_indent(&mut self) {
        self.writer.decrease_indent();
    }

    // =========================================================================
    // Identifier Helpers
    // =========================================================================

    pub(super) fn has_identifier_text(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        self.arena.get_identifier(node).is_some()
    }

    pub(super) fn write_identifier_text(&mut self, idx: NodeIndex) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };
        if let Some(ident) = self.arena.get_identifier(node) {
            self.write(&ident.escaped_text);
        }
    }

    /// Get identifier text from a node index
    pub(super) fn get_identifier_text(&self, idx: NodeIndex) -> String {
        let Some(node) = self.arena.get(idx) else {
            return String::new();
        };
        if let Some(ident) = self.arena.get_identifier(node) {
            return ident.escaped_text.clone();
        }
        String::new()
    }

    // =========================================================================
    // Unique Name Generation (mirrors TypeScript's makeUniqueName)
    // =========================================================================

    /// Generate a unique temp name that doesn't collide with any identifier in the source file
    /// or any previously generated temp name. Uses a single global counter like TypeScript.
    ///
    /// Generates names: _a, _b, _c, ..., _z, _0, _1, ...
    /// Skips names that appear in `file_identifiers` or `generated_temp_names`.
    pub(super) fn make_unique_name(&mut self) -> String {
        loop {
            let counter = self.ctx.destructuring_state.temp_var_counter;
            let name = if counter < 26 {
                format!("_{}", (b'a' + counter as u8) as char)
            } else {
                format!("_{}", counter - 26)
            };
            self.ctx.destructuring_state.temp_var_counter += 1;

            if !self.file_identifiers.contains(&name) && !self.generated_temp_names.contains(&name)
            {
                self.generated_temp_names.insert(name.clone());
                return name;
            }
            // Name collides, try next
        }
    }

    // =========================================================================
    // Emitter Helpers
    // =========================================================================

    pub(super) fn emit_comma_separated(&mut self, nodes: &[NodeIndex]) {
        let mut first = true;
        for &idx in nodes {
            if !first {
                self.write(", ");
            }
            first = false;
            self.emit(idx);
        }
    }

    pub(super) fn emit_heritage_expression(&mut self, idx: NodeIndex) {
        if idx.is_none() {
            return;
        }

        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if let Some(expr) = self.arena.get_expr_type_args(node) {
            // Emit the expression (e.g., Base or ns.Other)
            self.emit(expr.expression);
            // Emit type arguments only for ES5 targets.
            // For ES6+, type arguments should be erased since JavaScript
            // doesn't support generics at runtime.
            if self.ctx.target_es5 {
                if let Some(ref type_args) = expr.type_arguments {
                    if !type_args.nodes.is_empty() {
                        self.write("<");
                        self.emit_comma_separated(&type_args.nodes);
                        self.write(">");
                    }
                }
            }
        } else {
            self.emit(idx);
        }
    }

    // =========================================================================
    // Modifier Helpers
    // =========================================================================

    /// Check if modifiers include the `declare` keyword
    pub(super) fn has_declare_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        self.has_modifier(modifiers, SyntaxKind::DeclareKeyword as u16)
    }

    /// Check if modifiers include the `export` keyword
    pub(super) fn has_export_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        self.has_modifier(modifiers, SyntaxKind::ExportKeyword as u16)
    }

    /// Check if modifiers include the `default` keyword
    pub(super) fn has_default_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        self.has_modifier(modifiers, SyntaxKind::DefaultKeyword as u16)
    }

    /// Scan forward from `pos` past whitespace and comments to find the actual
    /// token start. Used because node.pos includes leading trivia.
    pub(super) fn skip_trivia_forward(&self, start: u32, end: u32) -> u32 {
        let Some(text) = self.source_text else {
            return start;
        };
        let bytes = text.as_bytes();
        let mut pos = start as usize;
        let end = std::cmp::min(end as usize, bytes.len());
        while pos < end {
            match bytes[pos] {
                b' ' | b'\t' | b'\r' | b'\n' => pos += 1,
                b'/' if pos + 1 < end && bytes[pos + 1] == b'/' => {
                    // Single-line comment: skip to end of line
                    pos += 2;
                    while pos < end && bytes[pos] != b'\n' && bytes[pos] != b'\r' {
                        pos += 1;
                    }
                }
                b'/' if pos + 1 < end && bytes[pos + 1] == b'*' => {
                    // Multi-line comment: skip to */
                    pos += 2;
                    while pos + 1 < end {
                        if bytes[pos] == b'*' && bytes[pos + 1] == b'/' {
                            pos += 2;
                            break;
                        }
                        pos += 1;
                    }
                }
                _ => break,
            }
        }
        pos as u32
    }

    /// Check if modifiers include a specific keyword
    pub(super) fn has_modifier(&self, modifiers: &Option<NodeList>, kind: u16) -> bool {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx)
                    && mod_node.kind == kind
                {
                    return true;
                }
            }
        }
        false
    }

    /// Check if the source text has a trailing comma after the last element
    /// in a list (object literal, array literal, etc.)
    ///
    /// Scans backwards from the closing bracket/brace to find if there's a
    /// comma before it (skipping whitespace). The parser includes the trailing
    /// comma in the last element's `end` position, so we scan backwards from
    /// the container's closing delimiter instead.
    pub(super) fn has_trailing_comma_in_source(
        &self,
        container: &tsz_parser::parser::node::Node,
        _elements: &[NodeIndex],
    ) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };

        let end = std::cmp::min(container.end as usize, text.len());
        if end == 0 {
            return false;
        }

        let bytes = text.as_bytes();

        // Find the closing bracket/brace by scanning backwards from the container end
        let mut pos = end;
        while pos > 0 {
            pos -= 1;
            match bytes[pos] {
                b'}' | b']' | b')' => break,
                _ => continue,
            }
        }

        // Scan backwards from the closing bracket to find comma (skipping whitespace)
        while pos > 0 {
            pos -= 1;
            match bytes[pos] {
                b',' => return true,
                b' ' | b'\t' | b'\r' | b'\n' => continue,
                _ => return false,
            }
        }
        false
    }
}
