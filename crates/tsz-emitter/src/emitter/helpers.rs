use super::Printer;
use crate::printer::safe_slice;
use crate::source_writer::SourcePosition;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(super) fn take_pending_source_pos(&mut self) -> Option<SourcePosition> {
        self.pending_source_pos.take()
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
            self.write_identifier(&ident.escaped_text);
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

    /// Save the current temp naming state and start a fresh scope.
    /// Used when entering a function to reset temp names (_a, _b, etc.)
    /// since each function scope has its own temp naming.
    pub(super) fn push_temp_scope(&mut self) {
        let saved_counter = self.ctx.destructuring_state.temp_var_counter;
        let saved_names = std::mem::take(&mut self.generated_temp_names);
        let saved_for_of = self.first_for_of_emitted;
        self.temp_scope_stack
            .push((saved_counter, saved_names, saved_for_of));
        self.ctx.destructuring_state.temp_var_counter = 0;
        self.first_for_of_emitted = false;
    }

    /// Restore the previous temp naming state when leaving a function scope.
    pub(super) fn pop_temp_scope(&mut self) {
        if let Some((counter, names, for_of)) = self.temp_scope_stack.pop() {
            self.ctx.destructuring_state.temp_var_counter = counter;
            self.generated_temp_names = names;
            self.first_for_of_emitted = for_of;
        }
    }

    /// Generate a unique temp name that doesn't collide with any identifier in the source file
    /// or any previously generated temp name. Uses a single global counter like TypeScript.
    ///
    /// Generates names: _a, _b, _c, ..., _z, _0, _1, ...
    /// Skips counts 8 (_i) and 13 (_n) which TypeScript reserves for dedicated TempFlags.
    /// Also skips names that appear in `file_identifiers` or `generated_temp_names`.
    pub(super) fn make_unique_name(&mut self) -> String {
        loop {
            let counter = self.ctx.destructuring_state.temp_var_counter;
            self.ctx.destructuring_state.temp_var_counter += 1;

            // TypeScript skips counts 8 (_i) and 13 (_n) - these are reserved for
            // dedicated TempFlags._i and TempFlags._n used by specific transforms
            if counter < 26 && (counter == 8 || counter == 13) {
                continue;
            }

            let name = if counter < 26 {
                format!("_{}", (b'a' + counter as u8) as char)
            } else {
                format!("_{}", counter - 26)
            };

            if !self.file_identifiers.contains(&name) && !self.generated_temp_names.contains(&name)
            {
                self.generated_temp_names.insert(name.clone());
                return name;
            }
            // Name collides, try next
        }
    }

    /// Like make_unique_name but also records the temp for hoisting as a `var` declaration.
    /// Used for assignment destructuring temps which need `var _a, _b, ...;` at scope top.
    pub(super) fn make_unique_name_hoisted(&mut self) -> String {
        let name = self.make_unique_name();
        self.hoisted_assignment_temps.push(name.clone());
        name
    }

    // =========================================================================
    // Emitter Helpers
    // =========================================================================

    pub(super) fn emit_comma_separated(&mut self, nodes: &[NodeIndex]) {
        let mut first = true;
        let mut prev_end: Option<u32> = None;
        for &idx in nodes {
            if !first {
                self.write(", ");
            }
            // Emit comments between the previous node/comma and this node.
            // This handles comments like: func(a, /*comment*/ b, c) or func(/*c*/ a)
            if let Some(node) = self.arena.get(idx) {
                let _range_start = prev_end.unwrap_or(node.pos); // For first node, this won't emit anything
                if prev_end.is_some() {
                    // For non-first nodes, emit comments between previous node end and current node start
                    self.emit_unemitted_comments_between(
                        prev_end.expect("prev_end is Some, checked by if condition"),
                        node.pos,
                    );
                }
            }
            first = false;
            if let Some(node) = self.arena.get(idx) {
                prev_end = Some(node.end);
            }
            self.emit(idx);
        }
    }

    /// Emit comments between two positions that haven't been emitted yet.
    /// This is used for comments in expression contexts (e.g., between function arguments).
    pub(crate) fn emit_unemitted_comments_between(&mut self, from_pos: u32, to_pos: u32) {
        if self.ctx.options.remove_comments {
            return;
        }

        let Some(text) = self.source_text else {
            return;
        };

        // Scan through all_comments to find ones in range [from_pos, to_pos)
        // that come after the current comment_emit_idx position.
        // We use a temporary index to scan without modifying comment_emit_idx,
        // since we're looking for comments that may be ahead of the current
        // emission position.
        let mut scan_idx = self.comment_emit_idx;
        while scan_idx < self.all_comments.len() {
            let c = &self.all_comments[scan_idx];
            if c.pos >= from_pos && c.end <= to_pos {
                // Found a comment in our range - emit it
                let comment_text = safe_slice::slice(text, c.pos as usize, c.end as usize);
                if !comment_text.is_empty() {
                    self.write(comment_text);
                    self.write_space();
                }
                // Advance the main index past this comment
                self.comment_emit_idx = scan_idx + 1;
                scan_idx += 1;
            } else if c.end <= from_pos {
                // Comment is before our range - already handled by statement-level emission
                scan_idx += 1;
            } else if c.pos >= to_pos {
                // Comment is past our target position, stop scanning
                break;
            } else {
                // Comment overlaps with range boundaries, skip it
                scan_idx += 1;
            }
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

    /// Scan forward from `pos` past whitespace only (preserving comments).
    /// Used to find the start of a statement while preserving comments
    /// that may belong to nested expressions.
    pub fn skip_whitespace_forward(&self, start: u32, end: u32) -> u32 {
        let Some(text) = self.source_text else {
            return start;
        };
        let bytes = text.as_bytes();
        let mut pos = start as usize;
        let end = std::cmp::min(end as usize, bytes.len());
        while pos < end {
            match bytes[pos] {
                b' ' | b'\t' | b'\r' | b'\n' => pos += 1,
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
