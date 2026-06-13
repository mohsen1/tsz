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

use crate::parser::node::NodeArenaPoolLengths;
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
    /// Lengths of every typed data pool at checkpoint time. Without this,
    /// failed speculations leave orphaned pool entries (identifiers, `type_refs`,
    /// etc.) that inflate peak memory and degrade cache efficiency on files with
    /// many recursive/generic types.
    arena_pool_lengths: NodeArenaPoolLengths,
    /// Scanner diagnostic high-water mark at checkpoint time. Restoring this
    /// ensures the position-dedup logic in `parse_error_at` sees the correct
    /// "lastError" tail after rollback rather than a stale post-speculation mark.
    scanner_diagnostics_high_water_mark: usize,
    deferred_module_close_braces: u32,
    abort_intersection_continuation: bool,
    fallback_import_type_options_once: bool,
    in_import_type_options_context: bool,
    import_attribute_tail_recovered: bool,
    suppress_object_literal_comma_once: bool,
    abort_object_literal_recovery_once: bool,
    recovered_template_literal_property_in_object: bool,
    recovered_object_literal_dot_tail_once: bool,
    suppress_next_missing_close_paren_error_once: bool,
    abort_function_signature_after_definite_assignment_tail_once: bool,
    recovered_definite_assignment_empty_statement_close_brace_pos: Option<u32>,
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
            arena_pool_lengths: self.arena.pool_checkpoint(),
            scanner_diagnostics_high_water_mark: self.scanner_diagnostics_high_water_mark,
            deferred_module_close_braces: self.deferred_module_close_braces,
            abort_intersection_continuation: self.abort_intersection_continuation,
            fallback_import_type_options_once: self.fallback_import_type_options_once,
            in_import_type_options_context: self.in_import_type_options_context,
            import_attribute_tail_recovered: self.import_attribute_tail_recovered,
            suppress_object_literal_comma_once: self.suppress_object_literal_comma_once,
            abort_object_literal_recovery_once: self.abort_object_literal_recovery_once,
            recovered_template_literal_property_in_object: self
                .recovered_template_literal_property_in_object,
            recovered_object_literal_dot_tail_once: self.recovered_object_literal_dot_tail_once,
            suppress_next_missing_close_paren_error_once: self
                .suppress_next_missing_close_paren_error_once,
            abort_function_signature_after_definite_assignment_tail_once: self
                .abort_function_signature_after_definite_assignment_tail_once,
            recovered_definite_assignment_empty_statement_close_brace_pos: self
                .recovered_definite_assignment_empty_statement_close_brace_pos,
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
            arena_pool_lengths,
            scanner_diagnostics_high_water_mark,
            deferred_module_close_braces,
            abort_intersection_continuation,
            fallback_import_type_options_once,
            in_import_type_options_context,
            import_attribute_tail_recovered,
            suppress_object_literal_comma_once,
            abort_object_literal_recovery_once,
            recovered_template_literal_property_in_object,
            recovered_object_literal_dot_tail_once,
            suppress_next_missing_close_paren_error_once,
            abort_function_signature_after_definite_assignment_tail_once,
            recovered_definite_assignment_empty_statement_close_brace_pos,
            saw_arrow_parameter_recovery,
        } = checkpoint;

        self.scanner.restore_state(scanner);
        self.current_token = current_token;
        self.context_flags = context_flags;
        self.last_error_pos = last_error_pos;
        self.parse_diagnostics.truncate(parse_diagnostics_len);
        self.arena.nodes.truncate(arena_nodes_len);
        self.arena.extended_info.truncate(arena_extended_info_len);
        self.arena.restore_pool_checkpoint(&arena_pool_lengths);
        self.scanner_diagnostics_high_water_mark = scanner_diagnostics_high_water_mark;
        self.deferred_module_close_braces = deferred_module_close_braces;
        self.abort_intersection_continuation = abort_intersection_continuation;
        self.fallback_import_type_options_once = fallback_import_type_options_once;
        self.in_import_type_options_context = in_import_type_options_context;
        self.import_attribute_tail_recovered = import_attribute_tail_recovered;
        self.suppress_object_literal_comma_once = suppress_object_literal_comma_once;
        self.abort_object_literal_recovery_once = abort_object_literal_recovery_once;
        self.recovered_template_literal_property_in_object =
            recovered_template_literal_property_in_object;
        self.recovered_object_literal_dot_tail_once = recovered_object_literal_dot_tail_once;
        self.suppress_next_missing_close_paren_error_once =
            suppress_next_missing_close_paren_error_once;
        self.abort_function_signature_after_definite_assignment_tail_once =
            abort_function_signature_after_definite_assignment_tail_once;
        self.recovered_definite_assignment_empty_statement_close_brace_pos =
            recovered_definite_assignment_empty_statement_close_brace_pos;
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
    use crate::parser::node::IdentifierData;
    use tsz_common::interner::AstAtom;
    use tsz_scanner::SyntaxKind;

    fn fresh_parser(source: &str) -> ParserState {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        parser.next_token();
        parser
    }

    fn make_test_identifier(text: &str) -> IdentifierData {
        IdentifierData {
            atom: AstAtom::NONE,
            escaped_text: text.to_string(),
            original_text: None,
        }
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
        let identifiers_len_before = parser.arena.identifiers.len();
        let type_refs_len_before = parser.arena.type_refs.len();
        let hwm_before = parser.scanner_diagnostics_high_water_mark;

        parser.speculate(|_| ());

        assert_eq!(parser.current_token, token_before);
        assert_eq!(parser.scanner.save_state().pos, pos_before);
        assert_eq!(parser.context_flags, context_before);
        assert_eq!(parser.last_error_pos, last_err_before);
        assert_eq!(parser.parse_diagnostics.len(), diag_len_before);
        assert_eq!(parser.arena.nodes.len(), nodes_len_before);
        assert_eq!(parser.arena.extended_info.len(), ext_len_before);
        assert_eq!(parser.arena.identifiers.len(), identifiers_len_before);
        assert_eq!(parser.arena.type_refs.len(), type_refs_len_before);
        assert_eq!(parser.scanner_diagnostics_high_water_mark, hwm_before);
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
            p.abort_object_literal_recovery_once = true;
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
        assert!(!parser.abort_object_literal_recovery_once);
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

    /// Arena typed-pool lengths are restored on rollback — no orphaned entries.
    ///
    /// Before this fix, `restore_speculation_checkpoint` only truncated
    /// `arena.nodes` and `arena.extended_info`. Every typed pool (identifiers,
    /// `type_refs`, etc.) retained entries created during a failed speculation,
    /// causing unbounded memory growth and cache degradation in files with many
    /// complex generic types. This test verifies that rollback also reclaims
    /// typed pool allocations.
    #[test]
    fn speculation_rollback_reclaims_typed_pool_entries() {
        let mut parser = fresh_parser("a b c");

        let idents_before = parser.arena.identifiers.len();
        let type_refs_before = parser.arena.type_refs.len();
        let nodes_before = parser.arena.nodes.len();

        parser.speculate(|p| {
            p.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                0,
                1,
                make_test_identifier("T"),
            );
        });

        // After rollback, typed pools must be at pre-speculation lengths.
        assert_eq!(
            parser.arena.identifiers.len(),
            idents_before,
            "identifiers pool leaked after speculation rollback"
        );
        assert_eq!(
            parser.arena.type_refs.len(),
            type_refs_before,
            "type_refs pool leaked after speculation rollback"
        );
        assert_eq!(
            parser.arena.nodes.len(),
            nodes_before,
            "nodes leaked after speculation rollback"
        );
    }

    /// Pool rollback is stable across nested speculations: inner rollback does
    /// not undo outer committed allocations, and outer rollback undoes both.
    #[test]
    fn nested_speculation_pool_rollback_is_correct() {
        let mut parser = fresh_parser("a b c d");

        let outer_idents_before = parser.arena.identifiers.len();

        // Outer speculation: one committed identifier, one nested failure.
        let outer_checkpoint = parser.speculation_checkpoint();

        // Commit an identifier at the outer level (this stays after outer commit).
        parser.arena.add_identifier(
            SyntaxKind::Identifier as u16,
            0,
            1,
            make_test_identifier("Outer"),
        );
        let after_outer_add = parser.arena.identifiers.len();

        // Inner speculation: allocate then roll back.
        parser.speculate(|p| {
            p.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                1,
                2,
                make_test_identifier("Inner"),
            );
        });

        // Inner rollback must restore to after-outer-add length.
        assert_eq!(
            parser.arena.identifiers.len(),
            after_outer_add,
            "inner rollback undid outer allocation"
        );

        // Outer rollback restores everything including the outer allocation.
        parser.restore_speculation_checkpoint(outer_checkpoint);
        assert_eq!(
            parser.arena.identifiers.len(),
            outer_idents_before,
            "outer rollback did not restore to pre-speculation length"
        );
    }
}
