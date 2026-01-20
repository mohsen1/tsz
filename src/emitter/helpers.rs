use super::Printer;
use crate::parser::{NodeIndex, NodeList};
use crate::scanner::SyntaxKind;
use crate::source_writer::SourcePosition;
use crate::printer::safe_slice;

impl<'a> Printer<'a> {
    fn take_pending_source_pos(&mut self) -> Option<SourcePosition> {
        self.pending_source_pos.take()
    }

    // =========================================================================
    // Safe String Slicing Helpers
    // =========================================================================

    /// Safely get a slice of text, returning empty string if out of bounds.
    /// This prevents panics from invalid string indices.
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
            self.emit(expr.expression);
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

    /// Check if modifiers include a specific keyword
    pub(super) fn has_modifier(&self, modifiers: &Option<NodeList>, kind: u16) -> bool {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    if mod_node.kind == kind {
                        return true;
                    }
                }
            }
        }
        false
    }
}
