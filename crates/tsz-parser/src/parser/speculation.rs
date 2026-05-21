//! Reusable speculation guard for parser-state checkpoints.
//!
//! Cheap single- or multi-token lookaheads use `look_ahead_is` (see
//! `parse_rules/utils.rs`), which checkpoints only the scanner. This module
//! provides a heavier guard for speculations that invoke a real `parse_*`
//! routine, which can mutate the scanner, the current token, parser context
//! flags, the diagnostic buffer, the AST arena, and a cluster of one-shot
//! recovery flags. The full field set lives on [`ParserCheckpoint`].
//!
//! Capture a [`ParserCheckpoint`] at the start of the speculation, then
//! either drop it (commit) or call [`ParserState::restore_speculation_checkpoint`]
//! (roll back). [`ParserState::speculate`] wraps the roll-back pattern in a
//! closure.

use tsz_scanner::SyntaxKind;
use tsz_scanner::scanner_impl::ScannerSnapshot;

use crate::parser::state::ParserState;

/// Snapshot of every parser-state field that a speculative `parse_*` call is
/// allowed to mutate. See the module docs for the field set.
pub(crate) struct ParserCheckpoint {
    scanner: ScannerSnapshot,
    current_token: SyntaxKind,
    context_flags: u32,
    last_error_pos: u32,
    parse_diagnostics_len: usize,
    arena_nodes_len: usize,
    arena_extended_info_len: usize,
    deferred_module_close_braces: u32,
    abort_intersection_continuation: bool,
    fallback_import_type_options_once: bool,
    in_import_type_options_context: bool,
    import_attribute_tail_recovered: bool,
    suppress_object_literal_comma_once: bool,
    suppress_next_missing_close_paren_error_once: bool,
    saw_arrow_parameter_recovery: bool,
}

impl ParserState {
    /// Capture a [`ParserCheckpoint`] for full speculation. Pair with
    /// [`Self::restore_speculation_checkpoint`] to roll back, or drop the
    /// checkpoint to commit.
    pub(crate) fn speculation_checkpoint(&self) -> ParserCheckpoint {
        ParserCheckpoint {
            scanner: self.scanner.save_state(),
            current_token: self.current_token,
            context_flags: self.context_flags,
            last_error_pos: self.last_error_pos,
            parse_diagnostics_len: self.parse_diagnostics.len(),
            arena_nodes_len: self.arena.nodes.len(),
            arena_extended_info_len: self.arena.extended_info.len(),
            deferred_module_close_braces: self.deferred_module_close_braces,
            abort_intersection_continuation: self.abort_intersection_continuation,
            fallback_import_type_options_once: self.fallback_import_type_options_once,
            in_import_type_options_context: self.in_import_type_options_context,
            import_attribute_tail_recovered: self.import_attribute_tail_recovered,
            suppress_object_literal_comma_once: self.suppress_object_literal_comma_once,
            suppress_next_missing_close_paren_error_once: self
                .suppress_next_missing_close_paren_error_once,
            saw_arrow_parameter_recovery: self.saw_arrow_parameter_recovery,
        }
    }

    /// Roll the parser back to the state captured by `checkpoint`.
    pub(crate) fn restore_speculation_checkpoint(&mut self, checkpoint: ParserCheckpoint) {
        let ParserCheckpoint {
            scanner,
            current_token,
            context_flags,
            last_error_pos,
            parse_diagnostics_len,
            arena_nodes_len,
            arena_extended_info_len,
            deferred_module_close_braces,
            abort_intersection_continuation,
            fallback_import_type_options_once,
            in_import_type_options_context,
            import_attribute_tail_recovered,
            suppress_object_literal_comma_once,
            suppress_next_missing_close_paren_error_once,
            saw_arrow_parameter_recovery,
        } = checkpoint;

        self.scanner.restore_state(scanner);
        self.current_token = current_token;
        self.context_flags = context_flags;
        self.last_error_pos = last_error_pos;
        self.parse_diagnostics.truncate(parse_diagnostics_len);
        self.arena.nodes.truncate(arena_nodes_len);
        self.arena.extended_info.truncate(arena_extended_info_len);
        self.deferred_module_close_braces = deferred_module_close_braces;
        self.abort_intersection_continuation = abort_intersection_continuation;
        self.fallback_import_type_options_once = fallback_import_type_options_once;
        self.in_import_type_options_context = in_import_type_options_context;
        self.import_attribute_tail_recovered = import_attribute_tail_recovered;
        self.suppress_object_literal_comma_once = suppress_object_literal_comma_once;
        self.suppress_next_missing_close_paren_error_once =
            suppress_next_missing_close_paren_error_once;
        self.saw_arrow_parameter_recovery = saw_arrow_parameter_recovery;
    }

    /// Run `body` as a roll-back-only speculation. Every parser-state field
    /// captured by [`Self::speculation_checkpoint`] is restored after `body`
    /// returns.
    pub(crate) fn speculate<R>(&mut self, body: impl FnOnce(&mut Self) -> R) -> R {
        let checkpoint = self.speculation_checkpoint();
        let result = body(self);
        self.restore_speculation_checkpoint(checkpoint);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_parser(source: &str) -> ParserState {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        parser.next_token();
        parser
    }

    /// A no-op speculation must leave every captured field unchanged.
    #[test]
    fn speculate_no_op_preserves_all_captured_fields() {
        let mut parser = fresh_parser("foo bar baz");
        let token_before = parser.current_token;
        let pos_before = parser.scanner.save_state().pos;
        let context_before = parser.context_flags;
        let last_err_before = parser.last_error_pos;
        let diag_len_before = parser.parse_diagnostics.len();
        let nodes_len_before = parser.arena.nodes.len();
        let ext_len_before = parser.arena.extended_info.len();

        parser.speculate(|_| ());

        assert_eq!(parser.current_token, token_before);
        assert_eq!(parser.scanner.save_state().pos, pos_before);
        assert_eq!(parser.context_flags, context_before);
        assert_eq!(parser.last_error_pos, last_err_before);
        assert_eq!(parser.parse_diagnostics.len(), diag_len_before);
        assert_eq!(parser.arena.nodes.len(), nodes_len_before);
        assert_eq!(parser.arena.extended_info.len(), ext_len_before);
    }

    /// Mutations performed inside the speculation body — scanner advance,
    /// context-flag toggles, recovery-flag flips, diagnostics pushed —
    /// must all be reverted on return.
    #[test]
    fn speculate_rolls_back_body_mutations() {
        let mut parser = fresh_parser("foo bar baz");
        let token_before = parser.current_token;
        let pos_before = parser.scanner.save_state().pos;
        let context_before = parser.context_flags;
        let last_err_before = parser.last_error_pos;

        parser.speculate(|p| {
            p.next_token();
            p.context_flags |= 0xFF;
            p.last_error_pos = 42;
            p.saw_arrow_parameter_recovery = true;
            p.deferred_module_close_braces = 7;
            p.abort_intersection_continuation = true;
            p.fallback_import_type_options_once = true;
            p.in_import_type_options_context = true;
            p.import_attribute_tail_recovered = true;
            p.suppress_object_literal_comma_once = true;
            p.suppress_next_missing_close_paren_error_once = true;
            p.parse_error_at_current_token("synthetic", 9999);
        });

        assert_eq!(parser.current_token, token_before);
        assert_eq!(parser.scanner.save_state().pos, pos_before);
        assert_eq!(parser.context_flags, context_before);
        assert_eq!(parser.last_error_pos, last_err_before);
        assert!(!parser.saw_arrow_parameter_recovery);
        assert_eq!(parser.deferred_module_close_braces, 0);
        assert!(!parser.abort_intersection_continuation);
        assert!(!parser.fallback_import_type_options_once);
        assert!(!parser.in_import_type_options_context);
        assert!(!parser.import_attribute_tail_recovered);
        assert!(!parser.suppress_object_literal_comma_once);
        assert!(!parser.suppress_next_missing_close_paren_error_once);
        assert_eq!(parser.parse_diagnostics.len(), 0);
    }

    /// The body's return value reaches the caller even though state rolls back.
    #[test]
    fn speculate_returns_body_value() {
        let mut parser = fresh_parser("foo");
        let value = parser.speculate(|_| 1234_u32);
        assert_eq!(value, 1234);
    }

    /// Explicit `restore_speculation_checkpoint` undoes mutations the same way
    /// the closure helper does. This is the lower-level API some sites prefer.
    #[test]
    fn restore_speculation_checkpoint_reverts_explicit_mutations() {
        let mut parser = fresh_parser("alpha beta");
        let token_before = parser.current_token;
        let pos_before = parser.scanner.save_state().pos;

        let checkpoint = parser.speculation_checkpoint();
        parser.next_token();
        parser.saw_arrow_parameter_recovery = true;
        parser.restore_speculation_checkpoint(checkpoint);

        assert_eq!(parser.current_token, token_before);
        assert_eq!(parser.scanner.save_state().pos, pos_before);
        assert!(!parser.saw_arrow_parameter_recovery);
    }
}
