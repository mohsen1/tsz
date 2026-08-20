//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-parser/src/parser/speculation.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 2e870c27ed685972fd7845a0a43d22bf47cbcd3cb98f18f7974a3c1c566c9d74 178 speculate_no_op_preserves_all_captured_fields
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
// TSZ_INLINE_TEST_END 2e870c27ed685972fd7845a0a43d22bf47cbcd3cb98f18f7974a3c1c566c9d74

// TSZ_INLINE_TEST_BEGIN 9f67f58e5126f35f2d0fb60309248cbe1ee9d8d58b120ee6bae56a0fdc7b4a64 209 speculate_rolls_back_body_mutations
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
// TSZ_INLINE_TEST_END 9f67f58e5126f35f2d0fb60309248cbe1ee9d8d58b120ee6bae56a0fdc7b4a64

// TSZ_INLINE_TEST_BEGIN fe187f4b7a63a7cb3b5abf4d2bc418bbe3ac27f35c75fdde1b2a707df336d224 250 speculate_returns_body_value
    /// The body's return value reaches the caller even though state rolls back.
    #[test]
    fn speculate_returns_body_value() {
        let mut parser = fresh_parser("foo");
        let value = parser.speculate(|_| 1234_u32);
        assert_eq!(value, 1234);
    }
// TSZ_INLINE_TEST_END fe187f4b7a63a7cb3b5abf4d2bc418bbe3ac27f35c75fdde1b2a707df336d224

// TSZ_INLINE_TEST_BEGIN a897ac7291033f260cbdc50da4a051d7beca3c3923d819d0446d18158c100aef 259 restore_speculation_checkpoint_reverts_explicit_mutations
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
// TSZ_INLINE_TEST_END a897ac7291033f260cbdc50da4a051d7beca3c3923d819d0446d18158c100aef

// TSZ_INLINE_TEST_BEGIN aa1c66c4dc21757de28a374ec833e60c55a8b0b639fc6310ed0155c29b546952 283 speculation_rollback_reclaims_typed_pool_entries
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
// TSZ_INLINE_TEST_END aa1c66c4dc21757de28a374ec833e60c55a8b0b639fc6310ed0155c29b546952

// TSZ_INLINE_TEST_BEGIN 3f99f2d2cdb5c90abf757d6b2e051482cad8f8ba6dd768f02698fd558781bbc5 320 nested_speculation_pool_rollback_is_correct
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
// TSZ_INLINE_TEST_END 3f99f2d2cdb5c90abf757d6b2e051482cad8f8ba6dd768f02698fd558781bbc5
