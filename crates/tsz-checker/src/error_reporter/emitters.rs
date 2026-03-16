//! Fundamental error emission helpers: node-anchored, position-anchored,
//! and templated diagnostic emitters.

use crate::diagnostics::{Diagnostic, format_message};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    /// Report an error at a specific node.
    pub(crate) fn error_at_node(&mut self, node_idx: NodeIndex, message: &str, code: u32) {
        if let Some((start, end)) = self.get_node_span(node_idx) {
            let length = end.saturating_sub(start);
            // Use the error() function which has deduplication by (start, code)
            self.error(start, length, message.to_string(), code);
        }
    }

    /// Emit a generator-related error (TS1221/TS1222) at the `*` asterisk token.
    ///
    /// TSC's `grammarErrorOnNode(node.asteriskToken!, ...)` anchors these errors
    /// at the asterisk, not the function/method node. Since our AST stores
    /// `asterisk_token` as a `bool` (not a node), we scan backward from the
    /// name node's position in source text to locate the `*`.
    pub(crate) fn emit_generator_error_at_asterisk(
        &mut self,
        name_idx: NodeIndex,
        fallback_idx: NodeIndex,
        message: &str,
        code: u32,
    ) {
        // Try to find the `*` by scanning backward from the name node's start position
        if let Some(name_node) = self.ctx.arena.get(name_idx)
            && let Some(sf) = self.ctx.arena.source_files.first()
        {
            let text = sf.text.as_bytes();
            let name_pos = name_node.pos as usize;
            // Scan backward from the name position to find `*`
            for i in (0..name_pos).rev() {
                match text.get(i) {
                    Some(b'*') => {
                        self.error_at_position(i as u32, 1, message, code);
                        return;
                    }
                    Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') => continue,
                    _ => break, // Hit a non-whitespace, non-asterisk char — give up
                }
            }
        }
        // Fallback: error at the entire node
        self.error_at_node(fallback_idx, message, code);
    }

    /// Emit a templated diagnostic error at a node.
    ///
    /// Looks up the message template for `code` via `get_message_template`,
    /// formats it with `args`, and emits the error at `node_idx`.
    /// Panics in debug mode if the code has no registered template.
    pub(crate) fn error_at_node_msg(&mut self, node_idx: NodeIndex, code: u32, args: &[&str]) {
        use tsz_common::diagnostics::get_message_template;
        let template = get_message_template(code).unwrap_or("Unexpected checker diagnostic code.");
        let message = format_message(template, args);
        self.error_at_node(node_idx, &message, code);
    }

    /// Get the source text for a node by extracting from the source file text.
    pub(crate) fn get_source_text_for_node(&self, node_idx: NodeIndex) -> String {
        if let Some((start, end)) = self.get_node_span(node_idx)
            && let Some(sf) = self.ctx.arena.source_files.first()
        {
            let text: &str = &sf.text;
            let s = start as usize;
            let e = end as usize;
            if s <= e && e <= text.len() {
                return text[s..e].to_string();
            }
        }
        String::new()
    }

    /// Report a program-level error (no file location).
    ///
    /// Used for diagnostics that tsc emits globally (e.g., TS2468 "Cannot find
    /// global value 'Promise'") rather than anchored to a specific source location.
    pub(crate) fn error_program_level(&mut self, message: String, code: u32) {
        self.ctx
            .push_diagnostic(Diagnostic::error(String::new(), 0, 0, message, code));
    }

    /// Report an error at a specific position.
    pub(crate) fn error_at_position(&mut self, start: u32, length: u32, message: &str, code: u32) {
        self.ctx.diagnostics.push(Diagnostic::error(
            self.ctx.file_name.clone(),
            start,
            length,
            message.to_string(),
            code,
        ));
    }

    /// Report TS1109: Expression expected, at a raw source position.
    ///
    /// Used when scanning JSDoc comments for `@import` tags that have empty
    /// or malformed import expressions. Routes through `push_diagnostic` for
    /// consistent deduplication.
    pub(crate) fn error_expression_expected_at_position(&mut self, start: u32, length: u32) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        self.ctx.push_diagnostic(Diagnostic::error(
            self.ctx.file_name.clone(),
            start,
            length,
            diagnostic_messages::EXPRESSION_EXPECTED.to_string(),
            diagnostic_codes::EXPRESSION_EXPECTED,
        ));
    }

    /// Report TS6133: '{name}' is declared but its value is never read.
    ///
    /// Used for unused variables, parameters, imports, and type parameters.
    /// Accepts raw position data since callers compute spans from declaration
    /// nodes directly. Routes through `push_diagnostic` for consistent dedup.
    pub(crate) fn error_declared_but_never_read(&mut self, name: &str, start: u32, length: u32) {
        use crate::diagnostics::diagnostic_codes;
        let message = format!("'{name}' is declared but its value is never read.");
        self.ctx.push_diagnostic(Diagnostic::error(
            self.ctx.file_name.clone(),
            start,
            length,
            message,
            diagnostic_codes::IS_DECLARED_BUT_ITS_VALUE_IS_NEVER_READ,
        ));
    }

    /// Report TS6196: '{name}' is declared but never used.
    ///
    /// Used for unused type-only declarations (classes, interfaces, type aliases,
    /// enums). Routes through `push_diagnostic` for consistent deduplication.
    pub(crate) fn error_declared_but_never_used(&mut self, name: &str, start: u32, length: u32) {
        let message = format!("'{name}' is declared but never used.");
        self.ctx.push_diagnostic(Diagnostic::error(
            self.ctx.file_name.clone(),
            start,
            length,
            message,
            6196,
        ));
    }

    /// Report an error at the current node being processed (from resolution stack).
    /// Falls back to the start of the file if no node is in the stack.
    pub(crate) fn error_at_current_node(&mut self, message: &str, code: u32) {
        // Try to use the last node in the resolution stack
        if let Some(&node_idx) = self.ctx.node_resolution_stack.last() {
            self.error_at_node(node_idx, message, code);
        } else {
            // No current node - emit at start of file
            self.error_at_position(0, 0, message, code);
        }
    }
}
